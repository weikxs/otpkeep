//! Native questions for sensitive actions (design/07-platform.md §1.3): shown by the system, so a web page
//! cannot answer them, and worded in the person's language.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::hotkeys::Hotkeys;

/// A question in Chinese and English: title, text and the button that goes ahead.
pub struct Question {
    pub zh: [&'static str; 3],
    pub en: [&'static str; 3],
}

/// Asks with Go / Cancel; true when the person goes ahead. Blocks: call it off the main thread.
pub fn confirm(app: &AppHandle, question: &Question) -> bool {
    let chinese = chinese(app);
    let [title, body, go] = if chinese { question.zh } else { question.en };
    let cancel = if chinese { "取消" } else { "Cancel" };
    app.dialog()
        .message(body)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            go.to_owned(),
            cancel.to_owned(),
        ))
        .blocking_show()
}

/// Whether the person reads Chinese: their setting, or the system's language.
pub fn chinese(app: &AppHandle) -> bool {
    let chosen = app
        .try_state::<Hotkeys>()
        .and_then(|h| h.settings().language);
    chosen
        .or_else(sys_locale::get_locale)
        .is_some_and(|language| language.to_lowercase().starts_with("zh"))
}

/// Exporting secrets in a file that is not encrypted.
pub const PLAIN_FILE: Question = Question {
    zh: [
        "导出未加密的文件",
        "这个文件包含密钥，没有加密。任何能读到它的人都能生成你的验证码。请只在需要时导出，用完删除。",
        "继续导出",
    ],
    en: [
        "Export an unencrypted file",
        "This file holds your secrets unencrypted. Anyone who can read it can make your codes. Export it only when needed and delete it afterwards.",
        "Export",
    ],
};

/// Showing QR codes that hold secrets.
pub const SHOW_CODES: Question = Question {
    zh: [
        "在屏幕上显示二维码",
        "二维码包含密钥。请确认周围没有人或摄像头能看到屏幕，也没有在录屏。",
        "显示",
    ],
    en: [
        "Show QR codes on the screen",
        "The QR codes hold your secrets. Make sure nobody and no camera can see the screen, and that nothing is recording it.",
        "Show",
    ],
};

/// Showing a pairing invite, which adds a device without the code.
pub const SHOW_INVITE: Question = Question {
    zh: [
        "显示配对二维码",
        "扫描这个二维码的设备可以直接加入你的同步组，获得全部账号。请只让你自己的设备扫描。",
        "显示",
    ],
    en: [
        "Show the pairing QR code",
        "A device that scans this code can join your sync group and receive every account. Let only your own devices scan it.",
        "Show",
    ],
};

/// Removing a member for good.
pub const REMOVE_DEVICE: Question = Question {
    zh: [
        "移除设备",
        "这台设备将被永久移出同步组，之后不能再与你的设备同步。它已有的数据无法远程删除；如果设备丢失，建议更换重要账号的两步验证密钥。",
        "移除",
    ],
    en: [
        "Remove the device",
        "The device leaves the sync group for good and can no longer sync with your devices. The data it has cannot be erased from here; if it was lost, consider replacing important two-factor keys.",
        "Remove",
    ],
};

/// Leaving the sync group.
pub const LEAVE_GROUP: Question = Question {
    zh: [
        "离开同步组",
        "本机保留全部账号，但不再与其他设备同步。离开无法通知其他设备，请之后在其他设备上移除本机。",
        "离开",
    ],
    en: [
        "Leave the sync group",
        "This device keeps every account but stops syncing. Other devices are not told; remove this device on one of them afterwards.",
        "Leave",
    ],
};
