//! Images in the terminal: a line of the page holding only a link to an image
//! file (`orgamnesia_core::images::parse`) is followed by the image, drawn with
//! the graphics of the terminal (kitty, sixel, iTerm2) or else in coloured half
//! blocks, when the page is read (not while it is edited: there the link is text).

use std::collections::HashMap;

use orgamnesia_core::images;
use ratatui::layout::Size;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui_image::{picker::Picker, protocol::Protocol, Resize};

/// Most rows an image takes.
const MAX_ROWS: u16 = 20;

/// What draws the images: `picker` is `None` until the terminal is asked what it
/// can draw (`tui::run`), so tests draw none unless they give one.
#[derive(Default)]
pub struct Images {
    pub picker: Option<Picker>,
    /// By file: the image read, and the last one fitted in cells; `None` when unreadable.
    cache: HashMap<String, Option<(image::DynamicImage, Fitted)>>,
}

/// An image fitted in cells, with their number.
type Fitted = Option<(Size, Protocol)>;

/// An image placed in the page: its first row (after the lines wrapped), its size in cells, its file.
pub struct Slot {
    pub row: usize,
    pub size: Size,
    pub file: String,
}

impl Images {
    /// The images of the page `page`, `text` drawn as `lines` (one per line of the
    /// text) in `width` columns: empty lines inserted under each image line for
    /// it, and where each image goes.
    pub fn place(&mut self, page: &str, text: &str, lines: &mut Vec<Line<'_>>, width: u16) -> Vec<Slot> {
        let Some(font) = self.picker.as_ref().map(|p| p.font_size()) else { return vec![] };
        let _format = orgamnesia_core::motion::FormatGuard::set(orgamnesia_core::Format::of_page(page));
        let text_lines: Vec<&str> = text.split('\n').collect();
        let mut found = vec![];
        for (i, line) in text_lines.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let Some(img) = images::parse(&chars).filter(|i| images::is_image(&i.target) && !i.target.contains("://")) else { continue };
            let file = images::resolve(page, &img.target);
            let Some(natural) = self.read(&file).map(|i| (i.width() as f64, i.height() as f64)) else { continue };
            let attr = i.checked_sub(1).and_then(|p| images::attr_width(text_lines[p]));
            let (fw, fh) = (font.width.max(1) as f64, font.height.max(1) as f64);
            let (w, h) = images::display_size(natural, attr, &img, width as f64 * fw);
            let size = Size::new(((w / fw).ceil() as u16).clamp(1, width), ((h / fh).ceil() as u16).clamp(1, MAX_ROWS));
            found.push((i, size, file));
        }
        // The rows under each image line, from the end so the indexes hold
        for (i, size, _) in found.iter().rev() {
            if *i < lines.len() { lines.splice(i + 1..i + 1, (0..size.height).map(|_| Line::default())); }
        }
        let rows = |l: &Line| Paragraph::new(l.clone()).wrap(Wrap { trim: false }).line_count(width).max(1);
        // `row`: the rows drawn down to `at` in `lines`; `shift`: the empty lines put above
        let (mut row, mut at, mut shift, mut slots) = (0, 0, 0, vec![]);
        for (i, size, file) in found {
            let index = i + shift;
            if index >= lines.len() { break; }
            row += lines[at..=index].iter().map(rows).sum::<usize>();
            slots.push(Slot { row, size, file });
            row += size.height as usize;
            shift += size.height as usize;
            at = index + 1 + size.height as usize;
        }
        slots
    }

    /// The image of the file, read once.
    fn read(&mut self, file: &str) -> Option<&image::DynamicImage> {
        self.cache.entry(file.to_string())
            .or_insert_with(|| image::ImageReader::open(file).ok()?.with_guessed_format().ok()?.decode().ok().map(|i| (i, None)))
            .as_ref().map(|(i, _)| i)
    }

    /// The image of `slot` fitted in its cells, made again when they change.
    pub fn protocol(&mut self, slot: &Slot) -> Option<&Protocol> {
        let picker = self.picker.as_ref()?;
        let (image, fitted) = self.cache.get_mut(&slot.file)?.as_mut()?;
        if fitted.as_ref().is_none_or(|(s, _)| *s != slot.size) {
            *fitted = picker.new_protocol(image.clone(), slot.size, Resize::Fit(None)).ok().map(|p| (slot.size, p));
        }
        fitted.as_ref().map(|(_, p)| p)
    }
}
