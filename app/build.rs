use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let source = "../design-system/tokens/tokens.json";
    println!("cargo:rerun-if-changed={source}");
    let tokens: serde_json::Value = serde_json::from_str(&fs::read_to_string(source)?)?;
    let mut output = String::new();
    for mode in ["light", "dark"] {
        output.push_str(&format!(
            "pub const {}: Palette = Palette {{\n",
            mode.to_uppercase()
        ));
        for (field, role) in [
            ("bg", "bg"),
            ("surface", "surface"),
            ("surface_raised", "surfaceRaised"),
            ("text", "text"),
            ("muted", "muted"),
            ("border", "border"),
            ("action", "action"),
            ("action_text", "actionText"),
            ("focus", "focus"),
            ("selected", "selected"),
            ("info_text", "infoText"),
            ("info_bg", "infoBg"),
            ("success_text", "successText"),
            ("warning_text", "warningText"),
            ("danger_text", "dangerText"),
            ("danger_bg", "dangerBg"),
        ] {
            let value = tokens["theme"][mode][role]
                .as_str()
                .ok_or("Missing theme role")?;
            let hex = if let Some(alias) = value
                .strip_prefix("{color.")
                .and_then(|s| s.strip_suffix('}'))
            {
                tokens["color"][alias]["$value"]
                    .as_str()
                    .ok_or("Missing color alias")?
            } else {
                value
            };
            let digits = hex.strip_prefix('#').ok_or("Expected hex color")?;
            if digits.len() != 6 || !digits.is_ascii() {
                return Err("Expected six-digit hex color".into());
            }
            let rgb = u32::from_str_radix(digits, 16)?;
            output.push_str(&format!(
                "{field}: Color32::from_rgb({}, {}, {}),\n",
                (rgb >> 16) & 255,
                (rgb >> 8) & 255,
                rgb & 255
            ));
        }
        output.push_str("};\n");
    }
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("palette.rs"),
        output,
    )?;
    Ok(())
}
