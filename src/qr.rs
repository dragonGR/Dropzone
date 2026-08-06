// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::DrawingArea;
use gtk4::prelude::*;
use qrcode::types::QrError;
use qrcode::{Color, QrCode};
use std::sync::Arc;

/// Generates a square boolean matrix from the URL payload.
/// `true` represents a dark module, `false` represents a light module.
pub fn generate_qr_matrix(url: &str) -> Result<Vec<Vec<bool>>, QrError> {
    let code = QrCode::new(url.as_bytes())?;
    let width = code.width();
    let mut matrix = Vec::with_capacity(width);

    for y in 0..width {
        let mut row = Vec::with_capacity(width);
        for x in 0..width {
            row.push(code[(x, y)] == Color::Dark);
        }
        matrix.push(row);
    }

    Ok(matrix)
}

/// Creates a GTK4 `DrawingArea` that renders the QR code using vector Cairo drawing.
/// Renders crisp lines at any DPI/scaling level with zero temporary files.
pub fn create_qr_widget(url: &str) -> Result<DrawingArea, QrError> {
    let matrix = Arc::new(generate_qr_matrix(url)?);
    let drawing_area = DrawingArea::builder()
        .content_width(220)
        .content_height(220)
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Center)
        .accessible_role(gtk4::AccessibleRole::Img)
        .build();

    drawing_area.update_property(&[gtk4::accessible::Property::Label(
        "QR code to access and download the shared file",
    )]);

    let qr_matrix = Arc::clone(&matrix);
    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let grid_size = qr_matrix.len();
        if grid_size == 0 {
            return;
        }

        let quiet_zone = 2.0;
        let total_units = grid_size as f64 + (quiet_zone * 2.0);

        let available_size = (width as f64).min(height as f64);
        let module_size = (available_size / total_units).floor().max(1.0);
        let total_pixel_size = total_units * module_size;

        let start_x = ((width as f64 - total_pixel_size) / 2.0).floor();
        let start_y = ((height as f64 - total_pixel_size) / 2.0).floor();

        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(start_x, start_y, total_pixel_size, total_pixel_size);
        let _ = cr.fill();

        cr.set_source_rgb(0.0, 0.0, 0.0);
        let modules_origin_x = start_x + (quiet_zone * module_size);
        let modules_origin_y = start_y + (quiet_zone * module_size);

        for (y, row) in qr_matrix.iter().enumerate() {
            for (x, &is_dark) in row.iter().enumerate() {
                if is_dark {
                    cr.rectangle(
                        modules_origin_x + (x as f64 * module_size),
                        modules_origin_y + (y as f64 * module_size),
                        module_size,
                        module_size,
                    );
                }
            }
        }
        let _ = cr.fill();
    });

    Ok(drawing_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_qr_matrix() {
        let url = "http://192.168.1.6:8080/s/0123456789abcdef";
        let matrix = generate_qr_matrix(url).expect("generate QR matrix");

        assert!(!matrix.is_empty());
        let width = matrix.len();
        assert!(width >= 21);
        for row in &matrix {
            assert_eq!(row.len(), width);
        }
    }
}
