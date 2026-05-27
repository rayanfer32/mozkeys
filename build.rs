use std::path::Path;

fn main() {
    let png_path = Path::new("src/assets/icon.png");
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = Path::new(&out_dir).join("icon.ico");
    let rc_path = Path::new(&out_dir).join("resources.rc");

    if png_path.exists() {
        // Read PNG data
        let png_data = std::fs::read(png_path).expect("Failed to read icon.png");
        let mut ico_data = Vec::new();

        // 1. ICO Header (6 bytes)
        ico_data.extend_from_slice(&[0, 0]); // Reserved
        ico_data.extend_from_slice(&[1, 0]); // Type (1 = icon)
        ico_data.extend_from_slice(&[1, 0]); // Number of images (1)

        // 2. Icon Directory Entry (16 bytes)
        ico_data.push(0); // Width (0 = 256px)
        ico_data.push(0); // Height (0 = 256px)
        ico_data.push(0); // Color palette (0 = no palette)
        ico_data.push(0); // Reserved
        ico_data.extend_from_slice(&[1, 0]); // Color planes (1)
        ico_data.extend_from_slice(&[32, 0]); // Bits per pixel (32)
        
        let size = png_data.len() as u32;
        ico_data.extend_from_slice(&size.to_le_bytes()); // Size of PNG data
        
        let offset = 22u32;
        ico_data.extend_from_slice(&offset.to_le_bytes()); // Offset to PNG data

        // 3. PNG Image Data
        ico_data.extend_from_slice(&png_data);

        // Write the ICO file
        std::fs::write(&ico_path, ico_data).expect("Failed to write icon.ico");
    }

    // Write a temporary resource script referencing the generated ICO
    let rc_content = format!(
        "1 ICON \"{}\"\n",
        ico_path.to_str().unwrap().replace("\\", "\\\\")
    );
    std::fs::write(&rc_path, rc_content).expect("Failed to write resources.rc");

    // Compile the resource script
    embed_resource::compile(&rc_path, &[] as &[&str]);
}
