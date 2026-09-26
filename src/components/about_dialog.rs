#![allow(non_snake_case)]
use crate::components::{DialogOverlay, LensBrand};
use crate::i18n;
use crate::state::Ctx;
use dioxus::prelude::*;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// oxvif version. Bumped manually when the Cargo.toml dependency moves —
/// `cargo metadata` lookup at runtime would mean shipping cargo, which
/// the single-binary release explicitly avoids.
pub(crate) const OXVIF_VERSION: &str = "0.16.0";
const REPO_URL: &str = "https://github.com/smiti1642/oxdm";
const SUPPORT_URL: &str = "https://buymeacoffee.com/smiti1642";

#[component]
pub fn AboutDialog(open: Signal<bool>) -> Element {
    if !*open.read() {
        return rsx! {};
    }
    let ctx = use_context::<Ctx>();
    let locale = *ctx.locale.read();

    rsx! {
        DialogOverlay {
            on_close: {
                let mut open = open;
                move |_| open.set(false)
            },
            inner_class: "dialog about-dialog".to_string(),

            div { class: "dialog-header",
                span { class: "dialog-title", {i18n::t(locale, "app_settings_about")} }
            }
                div { class: "dialog-body",
                    div { class: "about-icon", LensBrand { size: 72 } }
                    div { class: "about-name", "OxDM" }
                    div { class: "about-sub", {i18n::t(locale, "app_subtitle")} }
                    div { class: "about-tagline", {i18n::t(locale, "about_tagline")} }
                    div { class: "about-versions",
                        div { "OxDM v{APP_VERSION}" }
                        div { "oxvif v{OXVIF_VERSION}" }
                    }
                    button {
                        class: "btn btn-md btn-ghost",
                        onclick: move |_| {
                            if let Err(e) = opener::open_browser(SUPPORT_URL) {
                                tracing::warn!(error = %e, "open support page failed");
                            }
                        },
                        {i18n::t(locale, "about_support")}
                    }
                    div { class: "about-shortcuts",
                        div { class: "about-shortcuts-title", {i18n::t(locale, "about_shortcuts")} }
                        div { class: "about-shortcut",
                            span { class: "about-shortcut-keys",
                                kbd { "Ctrl" } "+" kbd { "F" }
                            }
                            span { class: "about-shortcut-desc", {i18n::t(locale, "shortcut_focus_search")} }
                        }
                        div { class: "about-shortcut",
                            span { class: "about-shortcut-keys", kbd { "F5" } }
                            span { class: "about-shortcut-desc", {i18n::t(locale, "shortcut_scan")} }
                        }
                        div { class: "about-shortcut",
                            span { class: "about-shortcut-keys",
                                kbd { "↑" } " / " kbd { "↓" }
                            }
                            span { class: "about-shortcut-desc", {i18n::t(locale, "shortcut_nav_devices")} }
                        }
                        div { class: "about-shortcut",
                            span { class: "about-shortcut-keys", kbd { "Esc" } }
                            span { class: "about-shortcut-desc", {i18n::t(locale, "shortcut_close_modal")} }
                        }
                    }
                }
                div { class: "dialog-footer about-footer",
                    button {
                        class: "btn btn-md btn-ghost",
                        onclick: move |_| {
                            if let Err(e) = opener::open_browser(REPO_URL) {
                                tracing::warn!(error = %e, "open repo failed");
                            }
                        },
                        {i18n::t(locale, "about_github")}
                    }
                    button {
                        class: "btn btn-md btn-primary",
                        onclick: {
                            let mut open = open;
                            move |_| open.set(false)
                        },
                        {i18n::t(locale, "btn_close")}
                    }
                }
        }
    }
}
