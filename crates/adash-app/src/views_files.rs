//! SFTP browser and editor. All filesystem operations use the SSH worker.
use crate::{app::Adash, widgets};
use adash_ssh::{RemoteFile, Request};
use gpui::{prelude::*, *};
use gpui_kit_material::prelude::*;

impl Adash {
    pub fn render_files_page(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.sessions.get(index).filter(|session| !session.closed) else {
            return widgets::empty_state("会话已关闭", "重新打开主机会话后浏览文件。");
        };
        let entry_count = session.files.as_ref().map(Vec::len);
        let connected = session.connected;
        let path_input = session.directory_input.clone();
        let error = session.file_error.clone();
        let editor = session.editor.clone();
        let editor_path = session.editor_path.clone();
        let dirty = self.editor_dirty(index, cx);
        let transfer = session.transfer;
        let files = match entry_count {
            Some(count) if count > 0 => uniform_list(
                SharedString::from(format!(
                    "remote-files-{index}-{}",
                    session.directory.display()
                )),
                count,
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    range
                        .filter_map(|row| {
                            let entry = this.sessions.get(index)?.files.as_ref()?.get(row)?.clone();
                            Some(remote_row(index, row, entry, connected, cx))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h_0()
            .into_any_element(),
            Some(_) => widgets::empty_state("目录为空", "可上传文件或新建目录。"),
            None => widgets::empty_state("尚未读取目录", "连接主机后刷新目录。"),
        };
        let font = self.store.data.settings.terminal_font_size;
        let family: SharedString = if self.store.data.settings.terminal_font.is_empty() {
            "monospace".into()
        } else {
            self.store.data.settings.terminal_font.clone().into()
        };
        let mut edit = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap(rem(10.));
        if let Some(path) = editor_path {
            edit = edit
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rem(8.))
                        .child(div().flex_1().min_w_0().text_size(TEXT_AUX).child(format!(
                            "{}{}",
                            if dirty { "● " } else { "" },
                            path.display()
                        )))
                        .child(
                            MaterialButton::new("remote-save")
                                .variant(ButtonVariant::Filled)
                                .disabled(!connected || !dirty)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.save_file(index, cx)),
                                )
                                .child("保存"),
                        ),
                )
                .child(
                    div()
                        .id("remote-editor-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(widgets::rounded_area_with_font(
                            "remote-editor",
                            &editor,
                            family,
                            f32::from(font),
                        )),
                );
        } else {
            edit = edit.child(widgets::empty_state(
                "文件编辑器",
                "选择一个 UTF-8 文本文件进行编辑。",
            ));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(rem(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rem(8.))
                    .child(
                        MaterialButton::new("remote-parent")
                            .variant(ButtonVariant::Tonal)
                            .disabled(!connected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.jump_directory(index, true, cx)
                            }))
                            .child("上一级"),
                    )
                    .child(div().flex_1().min_w_0().child(widgets::rounded_input(
                        "remote-path",
                        &path_input,
                        Some("路径"),
                    )))
                    .child(
                        MaterialButton::new("remote-go")
                            .variant(ButtonVariant::Tonal)
                            .disabled(!connected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.jump_directory(index, false, cx)
                            }))
                            .child("转到"),
                    )
                    .child(widgets::text_button(
                        "remote-refresh",
                        "刷新",
                        cx.listener(move |this, _, _, cx| this.refresh_files(index, cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(rem(8.))
                    .child(
                        MaterialButton::new("remote-upload")
                            .variant(ButtonVariant::Tonal)
                            .disabled(!connected || transfer.is_some())
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.upload_pick(index, cx)),
                            )
                            .child("上传文件"),
                    )
                    .child(
                        MaterialButton::new("remote-mkdir")
                            .variant(ButtonVariant::Tonal)
                            .disabled(!connected)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.begin_remote_name(index, None, window, cx)
                            }))
                            .child("新建目录"),
                    ),
            )
            .children(error.map(|error| widgets::notice(error, true)))
            .children(transfer.map(|(done, total)| {
                div()
                    .flex()
                    .items_center()
                    .gap(rem(10.))
                    .text_size(TEXT_AUX)
                    .child(format!("传输：{} / {} KiB", done / 1024, total / 1024))
                    .child(widgets::text_button(
                        "cancel-transfer",
                        "取消传输",
                        cx.listener(move |this, _, _, _| {
                            if let Some(worker) = this
                                .sessions
                                .get(index)
                                .and_then(|session| session.worker.as_ref())
                            {
                                worker.cancel_transfer();
                            }
                        }),
                    ))
            }))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap(rem(16.))
                    .child(div().flex().flex_col().w(rem(400.)).min_h_0().child(files))
                    .child(edit),
            )
            .into_any_element()
    }
}

fn remote_row(
    index: usize,
    row: usize,
    entry: RemoteFile,
    connected: bool,
    cx: &mut Context<Adash>,
) -> AnyElement {
    let key = format!("{index}-{row}");
    let path = entry.path;
    let open = path.clone();
    let rename = path.clone();
    let download = path.clone();
    let delete = path.clone();
    let directory = entry.directory;
    let size = if directory {
        "目录".into()
    } else {
        format!("{:.1} KiB", entry.size as f64 / 1024.)
    };
    div()
        .flex()
        .w_full()
        .min_w_0()
        .h(rem(64.))
        .flex_shrink_0()
        .items_center()
        .gap(rem(4.))
        .py(rem(4.))
        .child(
            div()
                .id(SharedString::from(format!("open-{key}")))
                .cursor_pointer()
                .flex_1()
                .min_w_0()
                .p(rem(6.))
                .rounded(rem(8.))
                .hover(|this| this.bg(color(ColorRole::SURFACE_CONTAINER_HIGH)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !connected {
                        return;
                    }
                    if directory {
                        if let Some(worker) = this
                            .sessions
                            .get(index)
                            .and_then(|session| session.worker.as_ref())
                        {
                            let _ = worker.send(Request::List(open.clone()));
                        }
                    } else {
                        this.open_file(index, open.clone(), cx);
                    }
                }))
                .child(div().text_size(TEXT_BODY).truncate().child(entry.name))
                .child(
                    div()
                        .text_size(TEXT_AUX)
                        .text_color(color(ColorRole::ON_SURFACE_VARIANT))
                        .child(size),
                ),
        )
        .child(
            MaterialButton::new(SharedString::from(format!("rename-{key}")))
                .variant(ButtonVariant::Text)
                .disabled(!connected)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_remote_name(index, Some(rename.clone()), window, cx)
                }))
                .child("重命名"),
        )
        .children((!directory).then(|| {
            MaterialButton::new(SharedString::from(format!("download-{key}")))
                .variant(ButtonVariant::Text)
                .disabled(!connected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.download_pick(index, download.clone(), cx)
                }))
                .child("下载")
        }))
        .child(
            MaterialButton::new(SharedString::from(format!("delete-{key}")))
                .variant(ButtonVariant::Text)
                .disabled(!connected)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pending_remote_delete = Some((index, delete.clone()));
                    cx.notify();
                }))
                .child("删除"),
        )
        .into_any_element()
}
