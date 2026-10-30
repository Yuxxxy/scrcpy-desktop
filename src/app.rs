use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, Hsla, IntoElement,
    ParentElement, Render, SharedString, Styled, Window, div, prelude::*, px,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    empty::{Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle},
    input::{Input, InputState},
    switch::Switch,
    table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
    tag::Tag,
};

use crate::config::{AppConfig, Device, load, save};
use crate::scrcpy::{Outcome, Session, launch};

pub struct AppView {
    config: AppConfig,
    selected: Option<usize>,
    form: Option<Rc<FormFields>>,
    form_is_new: bool,
    advanced_open: bool,
    audio_on: bool,
    on_top: bool,
    fullscreen: bool,
    toast: Option<SharedString>,
    session: Option<Session>,
    connecting: Option<(usize, Instant)>,
    focus_handle: FocusHandle,
}

struct FormFields {
    name: Entity<InputState>,
    host: Entity<InputState>,
    port: Entity<InputState>,
    extra: Entity<InputState>,
    max_size: Entity<InputState>,
    bit_rate: Entity<InputState>,
    max_fps: Entity<InputState>,
    title: Entity<InputState>,
}

fn numeric(text: &str) -> String {
    text.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

fn parse_opt_u32(raw: &str) -> Option<u32> {
    numeric(raw).parse::<u32>().ok().filter(|value| *value > 0)
}

fn parse_opt_string(raw: &str) -> Option<String> {
    let value = raw.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn make_state(
    placeholder: &'static str,
    value: String,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    cx.new(|cx| {
        let state = InputState::new(window, cx).placeholder(placeholder);
        if value.is_empty() {
            state
        } else {
            state.default_value(value)
        }
    })
}

fn take_text(field: &Entity<InputState>, cx: &App) -> String {
    field.read(cx).value().to_string()
}

fn label_row(title: &'static str, input: impl IntoElement, muted: Hsla) -> impl IntoElement {
    div()
        .v_flex()
        .gap_1()
        .flex_1()
        .min_w_0()
        .child(div().text_xs().text_color(muted).child(title))
        .child(input)
}

/// Builds a click handler that runs `action` against the app entity.
fn on_app(
    app: &Entity<AppView>,
    action: impl Fn(&mut AppView, &mut Window, &mut Context<AppView>) + 'static,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, window, cx| {
        app.update(cx, |app, cx| action(app, window, cx));
    }
}

/// Builds a change handler for controlled inputs (checkbox / switch).
fn on_toggle(
    app: &Entity<AppView>,
    action: impl Fn(&mut AppView, bool, &mut Context<AppView>) + 'static,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |checked, _, cx| {
        let checked = *checked;
        app.update(cx, |app, cx| action(app, checked, cx));
    }
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = load();
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let selected = if config.devices.is_empty() {
            None
        } else {
            Some(0)
        };
        Self {
            config,
            selected,
            form: None,
            form_is_new: true,
            advanced_open: false,
            audio_on: true,
            on_top: false,
            fullscreen: false,
            toast: None,
            session: None,
            connecting: None,
            focus_handle,
        }
    }

    fn build_form(
        &mut self,
        device: Option<&Device>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Rc<FormFields> {
        let empty = Device::default();
        let device = device.unwrap_or(&empty);

        self.audio_on = device.audio;
        self.on_top = device.always_on_top;
        self.fullscreen = device.fullscreen;

        Rc::new(FormFields {
            name: make_state("例如 小米 14", device.name.clone(), window, cx),
            host: make_state("192.168.1.10", device.host.clone(), window, cx),
            port: make_state("5555", device.port.to_string(), window, cx),
            extra: make_state(
                "--no-window-animation --max-size=1080",
                device.extra_args.clone(),
                window,
                cx,
            ),
            max_size: make_state(
                "如 1080",
                device.max_size.map(|v| v.to_string()).unwrap_or_default(),
                window,
                cx,
            ),
            bit_rate: make_state(
                "如 8M",
                device.bit_rate.clone().unwrap_or_default(),
                window,
                cx,
            ),
            max_fps: make_state(
                "如 60",
                device.max_fps.map(|v| v.to_string()).unwrap_or_default(),
                window,
                cx,
            ),
            title: make_state(
                "留空则使用设备名称",
                device.window_title.clone().unwrap_or_default(),
                window,
                cx,
            ),
        })
    }

    fn open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fields = self.build_form(None, window, cx);
        self.form = Some(fields);
        self.form_is_new = true;
        self.advanced_open = false;
        self.toast = None;
        cx.notify();
    }

    fn fill_form(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(device) = self.config.devices.get(index).cloned() else {
            return;
        };
        let fields = self.build_form(Some(&device), window, cx);
        self.form = Some(fields);
        self.form_is_new = false;
        self.selected = Some(index);
        self.toast = None;
        cx.notify();
    }

    fn save_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fields) = self.form.clone() else {
            return;
        };
        let host = take_text(&fields.host, cx).trim().to_string();
        let name = take_text(&fields.name, cx).trim().to_string();
        let port_raw = take_text(&fields.port, cx);
        let Some(port) = numeric(&port_raw)
            .parse::<u16>()
            .ok()
            .filter(|value| *value > 0)
        else {
            self.set_toast("端口需要是 1–65535 的数字", cx);
            return;
        };
        if host.is_empty() {
            self.set_toast("请填写设备 IP 或主机名", cx);
            return;
        }

        let editing_id = if self.form_is_new {
            None
        } else {
            self.selected.and_then(|index| self.config.devices.get(index)).map(|device| device.id)
        };

        let mut device = Device {
            id: editing_id.unwrap_or_default(),
            name: if name.is_empty() { host.clone() } else { name },
            host,
            port,
            audio: self.audio_on,
            always_on_top: self.on_top,
            fullscreen: self.fullscreen,
            max_size: parse_opt_u32(&take_text(&fields.max_size, cx)),
            bit_rate: parse_opt_string(&take_text(&fields.bit_rate, cx)),
            max_fps: parse_opt_u32(&take_text(&fields.max_fps, cx)),
            window_title: parse_opt_string(&take_text(&fields.title, cx)),
            extra_args: take_text(&fields.extra, cx).trim().to_string(),
        };

        if !self.config.devices.iter().any(|item| item.id == device.id) {
            device.id = self.config.devices.iter().map(|item| item.id).max().unwrap_or(0) + 1;
        }

        let saved_id = device.id;

        match self
            .config
            .devices
            .iter_mut()
            .find(|existing| existing.id == device.id)
        {
            Some(existing) => *existing = device,
            None => self.config.devices.push(device),
        }

        if let Err(error) = save(&self.config) {
            self.set_toast(&format!("写入配置失败：{error}"), cx);
            return;
        }

        self.form = None;
        self.selected = self.config.devices.iter().position(|item| item.id == saved_id);
        window.blur(cx);
        self.focus_handle.focus(window, cx);
        self.set_toast("已保存", cx);
    }

    fn close_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.form = None;
        window.blur(cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn connect_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(device) = self.config.devices.get(index).cloned() else {
            return;
        };
        self.selected = Some(index);
        self.toast = None;
        match launch(&device) {
            Ok(session) => {
                self.session = Some(session);
                self.connecting = Some((index, Instant::now()));
                self.poll_session(cx);
                cx.notify();
            }
            Err(error) => self.set_toast(&format!("启动失败：{error}"), cx),
        }
    }

    fn poll_session(&mut self, cx: &mut Context<Self>) {
        let entity = cx.weak_entity();
        cx.spawn(async move |_, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let Ok(continue_polling) = entity.update(cx, |app, cx| app.tick_session(cx)) else {
                    return;
                };
                if !continue_polling {
                    return;
                }
            }
        })
        .detach();
    }

    fn tick_session(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((index, started)) = self.connecting else {
            return false;
        };
        let address = self
            .config
            .devices
            .get(index)
            .map(|device| device.address())
            .unwrap_or_default();
        let outcome = self.session.as_ref().map(Session::outcome);
        match outcome {
            Some(Outcome::Ready) => {
                self.connecting = None;
                self.session = None;
                self.set_toast(&format!("已连接 · {address}"), cx);
                false
            }
            Some(Outcome::Failed(reason)) => {
                self.connecting = None;
                self.session = None;
                self.set_toast(&reason, cx);
                false
            }
            Some(Outcome::Running) => {
                if started.elapsed() > Duration::from_secs(90) {
                    self.connecting = None;
                    self.session = None;
                    self.set_toast(&format!("连接超时 · {address}"), cx);
                    return false;
                }
                true
            }
            None => false,
        }
    }

    fn delete_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(device) = self.config.devices.get(index).cloned() else {
            return;
        };
        self.config.devices.remove(index);
        let _ = save(&self.config);
        self.selected = if self.config.devices.is_empty() {
            None
        } else {
            Some(index.min(self.config.devices.len() - 1))
        };
        self.set_toast(&format!("已删除 {}", device.name), cx);
    }

    fn set_toast(&mut self, message: &str, cx: &mut Context<Self>) {
        self.toast = Some(SharedString::from(message.to_string()));
        cx.notify();
        let this = cx.weak_entity();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2600))
                .await;
            let _ = this.update(cx, |app, cx| {
                app.toast = None;
                cx.notify();
            });
        })
        .detach();
    }

    fn add_button(&self, id: &'static str, app: &Entity<AppView>) -> Button {
        Button::new(id)
            .primary()
            .small()
            .label("＋ 添加设备")
            .on_click(on_app(app, |app, window, cx| app.open_form(window, cx)))
    }

    fn render_header(&self, app: Entity<AppView>, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.config.devices.len();
        let muted = cx.theme().muted_foreground;
        let show_add = self.form.is_none();

        div()
            .h_flex()
            .flex_shrink_0()
            .justify_between()
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(div().text_lg().font_semibold().child("设备"))
                    .child(div().text_sm().text_color(muted).child(format!("{count} 台"))),
            )
            .child(div().when(show_add, |this| {
                this.child(self.add_button("header-add", &app))
            }))
    }

    fn render_row(
        &self,
        device: &Device,
        index: usize,
        app: Entity<AppView>,
        cx: &mut Context<Self>,
    ) -> TableRow {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let active = cx.theme().table_active;
        let selected = self.selected == Some(index);
        let id = device.id;

        TableRow::new()
            .when(selected, |row| row.bg(active))
            .child(TableCell::new().child(
                div()
                    .id(("row", index))
                    .h_flex()
                    .gap_1p5()
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(7.))
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(if selected { primary } else { muted }),
                    )
                    .child(
                        div()
                            .font_medium()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(device.name.clone()),
                    )
                    .on_click(on_app(&app, move |app, _, cx| {
                        app.selected = Some(index);
                        cx.notify();
                    })),
            ))
            .child(
                TableCell::new()
                    .w(px(124.))
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .whitespace_nowrap()
                            .child(device.address()),
                    ),
            )
            .child(
                TableCell::new()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .h_flex()
                            .gap_1()
                            .children(device.tags().into_iter().take(3).map(|tag| Tag::new().small().child(tag))),
                    ),
            )
            .child(
                TableCell::new()
                    .w(px(150.))
                    .flex_shrink_0()
                    .text_right()
                    .child(
                        div()
                            .h_flex()
                            .gap_1()
                            .justify_end()
                            .child(
                                Button::new(("connect", id))
                                    .primary()
                                    .xsmall()
                                    .label("连接")
                                    .on_click(on_app(&app, move |app, _, cx| {
                                        app.connect_at(index, cx);
                                    })),
                            )
                            .child(
                                Button::new(("edit", id))
                                    .ghost()
                                    .xsmall()
                                    .label("编辑")
                                    .on_click(on_app(&app, move |app, window, cx| {
                                        app.fill_form(index, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("delete", id))
                                    .ghost()
                                    .xsmall()
                                    .label("删除")
                                    .on_click(on_app(&app, move |app, _, cx| {
                                        app.delete_at(index, cx);
                                    })),
                            ),
                    ),
            )
    }

    fn render_table(&self, app: Entity<AppView>, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .config
            .devices
            .iter()
            .enumerate()
            .map(|(index, device)| self.render_row(device, index, app.clone(), cx))
            .collect::<Vec<_>>();

        div()
            .id("device-table")
            .v_flex()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                Table::new()
                    .small()
                    .child(TableHeader::new().child(
                        TableRow::new()
                            .child(TableHead::new().child("设备名称"))
                            .child(TableHead::new().w(px(124.)).child("地址"))
                            .child(TableHead::new().child("配置"))
                            .child(
                                TableHead::new()
                                    .w(px(150.))
                                    .text_right()
                                    .child("操作"),
                            ),
                    ))
                    .child(TableBody::new().children(rows)),
            )
    }

    fn render_empty(&self, app: Entity<AppView>) -> impl IntoElement {
        Empty::new()
            .header(
                EmptyHeader::new()
                    .title(EmptyTitle::new().child("还没有设备"))
                    .description(EmptyDescription::new().child(
                        "填写设备 IP 与端口，即可通过 scrcpy 镜像并控制这台安卓设备",
                    )),
            )
            .content(EmptyContent::new().child(self.add_button("empty-add", &app)))
    }

    fn render_form(
        &self,
        fields: Rc<FormFields>,
        is_new: bool,
        advanced: bool,
        app: Entity<AppView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let audio = self.audio_on;
        let on_top = self.on_top;
        let fullscreen = self.fullscreen;

        div()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .child(if is_new { "添加设备" } else { "编辑设备" }),
                    )
                    .child(
                        Button::new("form-close")
                            .ghost()
                            .xsmall()
                            .label("✕")
                            .on_click(on_app(&app, |app, window, cx| {
                                app.close_form(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(label_row(
                        "名称",
                        Input::new(&fields.name).small().w_full(),
                        muted,
                    ))
                    .child(
                        div()
                            .w(px(88.))
                            .flex_shrink_0()
                            .child(label_row(
                                "端口",
                                Input::new(&fields.port).small().w_full(),
                                muted,
                            )),
                    ),
            )
            .child(label_row(
                "IP / 主机",
                Input::new(&fields.host).small().w_full(),
                muted,
            ))
            .child(
                div()
                    .h_flex()
                    .gap_4()
                    .items_center()
                    .child(
                        Switch::new("form-audio")
                            .small()
                            .checked(audio)
                            .label("音频")
                            .on_change(on_toggle(&app, |app, checked, cx| {
                                app.audio_on = checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("form-on-top")
                            .small()
                            .checked(on_top)
                            .label("窗口置顶")
                            .on_change(on_toggle(&app, |app, checked, cx| {
                                app.on_top = checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("form-fullscreen")
                            .small()
                            .checked(fullscreen)
                            .label("全屏")
                            .on_change(on_toggle(&app, |app, checked, cx| {
                                app.fullscreen = checked;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("toggle-advanced")
                            .ghost()
                            .xsmall()
                            .label(if advanced { "收起 ▴" } else { "高级 ▾" })
                            .on_click(on_app(&app, |app, _, cx| {
                                app.advanced_open = !app.advanced_open;
                                cx.notify();
                            })),
                    ),
            )
            .child(label_row(
                "自定义参数",
                Input::new(&fields.extra).small().w_full(),
                muted,
            ))
            .when(advanced, |this| {
                this.child(
                    div()
                        .h_flex()
                        .gap_2()
                        .child(label_row(
                            "最大尺寸",
                            Input::new(&fields.max_size).small().w_full(),
                            muted,
                        ))
                        .child(label_row(
                            "码率",
                            Input::new(&fields.bit_rate).small().w_full(),
                            muted,
                        ))
                        .child(label_row(
                            "最大帧率",
                            Input::new(&fields.max_fps).small().w_full(),
                            muted,
                        )),
                )
                .child(label_row(
                    "窗口标题",
                    Input::new(&fields.title).small().w_full(),
                    muted,
                ))
            })
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .justify_end()
                    .pt_1()
                    .child(
                        Button::new("form-cancel")
                            .ghost()
                            .small()
                            .label("取消")
                            .on_click(on_app(&app, |app, window, cx| {
                                app.close_form(window, cx);
                            })),
                    )
                    .child(
                        Button::new("form-save")
                            .primary()
                            .small()
                            .label("保存")
                            .on_click(on_app(&app, |app, window, cx| {
                                app.save_form(window, cx);
                            })),
                    ),
            )
    }
}

impl Focusable for AppView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let app = cx.entity();
        let form = self.form.clone();
        let form_is_new = self.form_is_new;
        let advanced = self.advanced_open;
        let device_count = self.config.devices.len();
        let toast = self.toast.clone();

        let body: AnyElement = if let Some(fields) = form.clone() {
            div()
                .id("form-scroll")
                .v_flex()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(self.render_form(fields, form_is_new, advanced, app.clone(), cx))
                .into_any_element()
        } else if device_count == 0 {
            self.render_empty(app.clone()).into_any_element()
        } else {
            self.render_table(app.clone(), cx).into_any_element()
        };

        let status: Option<SharedString> = self
            .connecting
            .and_then(|(index, _)| self.config.devices.get(index))
            .map(|device| format!("正在推送 scrcpy-server · {}", device.address()).into())
            .or_else(|| toast.clone());

        div()
            .size_full()
            .v_flex()
            .bg(background)
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .p_3()
                    .when(form.is_none(), |this| this.child(self.render_header(app, cx)))
                    .child(body),
            )
            .when_some(status, |this, message| {
                this.child(
                    div()
                        .h_flex()
                        .justify_center()
                        .py_1p5()
                        .child(Tag::new().small().child(message)),
                )
            })
    }
}
