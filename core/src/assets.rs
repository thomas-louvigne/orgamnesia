//! Images added to a page: the files go to the `assets/` folder of the project.

/// Extensions of the files drawn as images.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "avif", "ico"];

/// Name of the file an image `name` is saved under in the assets folder, as Logseq
/// names them: its name, the time (ms) and `_0` (`photo_1759520598218_0.png`), what
/// is neither a letter, a digit, `-` nor `_` turned into `_`.
pub fn asset_name(name: &str, millis: u64) -> String {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "png"));
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let stem = if stem.is_empty() { "image".to_string() } else { stem };
    format!("{stem}_{millis}_0.{}", ext.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_are_named_as_logseq_names_them() {
        assert_eq!(asset_name("Photo de vacances.JPG", 42), "Photo_de_vacances_42_0.jpg");
        assert_eq!(asset_name("ramchard.png", 1759520598218), "ramchard_1759520598218_0.png");
    }
}
