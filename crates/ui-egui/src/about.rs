//! The About window every Nodalia app shows (`nodalia_theme::about`): the heading with its round
//! close button, then the page with Forja's icon, name and version, what it is, the suite it
//! belongs to and the links, in the engineering suite's faces. There are no Contributors or Models
//! tabs; the authors of the code this app began from are credited in `NOTICE`.

use egui::{Rect, TextureHandle};
use nodalia_theme::about as ndl;
use nodalia_theme::engineering::{self as eng, Look};

use crate::ndl_window;

const WEBSITE: &str = "https://getnodalia.com/forja";
const GITHUB: &str = "https://github.com/getnodalia/forja";
const ISSUES: &str = "https://github.com/getnodalia/forja/issues";
const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/dev.danielmiguelt.Forja.png");

pub fn show(ctx: &egui::Context, open: &mut bool) {
    let look = eng::about_look(&Look::new(crate::theme::ndl_palette()));
    let mut close = false;
    let mut link = None;
    ndl_window::Window::new("")
        .id(egui::Id::unique("About Forja"))
        .open(open)
        .title_bar(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            ui.set_width(ndl::measures::WIDTH);
            close = ndl::heading(ui, &look, crate::tl!("About Forja"), egui::Id::NULL.with("forja.about.close")).closed;
            let version = crate::tf!("Version {version}", version = env!("CARGO_PKG_VERSION"));
            let links = [
                ndl::Link { label: crate::tl!("Forja page"), url: WEBSITE },
                ndl::Link { label: crate::tl!("GitHub"), url: GITHUB },
                ndl::Link { label: crate::tl!("Report an issue"), url: ISSUES },
            ];
            let info = ndl::Info {
                name: "Forja",
                version: &version,
                tagline: crate::tl!("Computer-aided design and drafting: an open-source, clean-room CAD application written in pure Rust."),
                suite: crate::tl!("Part of the Nodalia suite of open engineering tools."),
                links: &links,
                footer: crate::tl!("Open source. No telemetry."),
            };
            let icon = icon(ui.ctx());
            let page = ndl::page(ui, &look, &info, |ui, rect: Rect| {
                if let Some(icon) = &icon {
                    egui::Image::new(icon).fit_to_exact_size(rect.size()).paint_at(ui, rect);
                }
            });
            if let Some(i) = page.picked {
                link = links.get(i).map(|l| l.url);
            }
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                let dim = ui.visuals().weak_text_color();
                ui.label(egui::RichText::new(crate::tl!("Not affiliated with Autodesk, Inc.")).small().color(dim));
            });
        });
    if let Some(url) = link {
        ctx.open_url(egui::OpenUrl::new_tab(url));
    }
    if close {
        *open = false;
    }
}

/// The app icon as a texture, decoded once.
pub(crate) fn icon(ctx: &egui::Context) -> Option<TextureHandle> {
    let id = egui::Id::unique("about_icon_texture");
    if let Some(t) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(t);
    }
    let rgba = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    // Mipmaps: the title bar draws it at 24 points, far below its 128 pixels.
    let options = egui::TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..egui::TextureOptions::LINEAR };
    let texture = ctx.load_texture("about_icon", image, options);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}
