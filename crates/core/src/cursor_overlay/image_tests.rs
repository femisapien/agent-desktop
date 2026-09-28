use super::*;
use crate::{Point, Rect};
use std::path::PathBuf;

const PNG_HEADER: &[u8] = b"\x89PNG\r\n\x1a\n";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ad-cursor-image-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_str().unwrap().to_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn origin() -> Point {
    Point { x: 0.0, y: 0.0 }
}

fn absolute(name: &str) -> String {
    std::env::temp_dir().join(name).to_str().unwrap().to_owned()
}

#[test]
fn image_paths_and_hotspots_are_validated() {
    assert!(CursorImage::new(absolute("cursor.png"), origin()).is_ok());
    assert!(CursorImage::new(absolute("cursor.PDF"), Point { x: 512.0, y: 4.0 }).is_ok());
    let long = absolute(&format!("{}.png", "a".repeat(MAX_CURSOR_IMAGE_PATH_BYTES)));
    let rejected = [
        (String::new(), origin()),
        ("cursor.png".to_owned(), origin()),
        (absolute("cur\0sor.png"), origin()),
        (absolute("cursor.svg"), origin()),
        (absolute("cursor"), origin()),
        (long, origin()),
        (absolute("cursor.png"), Point { x: -1.0, y: 0.0 }),
        (
            absolute("cursor.png"),
            Point {
                x: 0.0,
                y: f64::NAN,
            },
        ),
        (absolute("cursor.png"), Point { x: 513.0, y: 0.0 }),
    ];
    for (path, hotspot) in rejected {
        let error = CursorImage::new(path.clone(), hotspot).expect_err(&path);
        assert_eq!(error.code, crate::ErrorCode::InvalidArgs);
    }
}

#[test]
fn a_1024_byte_absolute_path_is_the_longest_accepted() {
    let base = absolute("x");
    let stem = MAX_CURSOR_IMAGE_PATH_BYTES - base.len() - ".png".len();
    let exact = format!("{base}{}.png", "a".repeat(stem));
    assert_eq!(exact.len(), MAX_CURSOR_IMAGE_PATH_BYTES);
    assert!(CursorImage::new(exact.clone(), origin()).is_ok());
    let over = format!("{base}{}.png", "a".repeat(stem + 1));
    assert!(CursorImage::new(over, origin()).is_err());
}

#[test]
fn verify_file_checks_signature_size_and_kind() {
    let dir = TempDir::new("verify");
    let png = dir.file("ok.png", &[PNG_HEADER, b"rest"].concat());
    assert!(
        CursorImage::new(png, origin())
            .unwrap()
            .verify_file()
            .is_ok()
    );
    let pdf = dir.file("ok.pdf", b"%PDF-1.7\n");
    assert!(
        CursorImage::new(pdf, origin())
            .unwrap()
            .verify_file()
            .is_ok()
    );
    let upper = dir.file("ok.PNG", PNG_HEADER);
    assert!(
        CursorImage::new(upper, origin())
            .unwrap()
            .verify_file()
            .is_ok()
    );
    let wrong = dir.file("fake.png", b"%PDF-1.7\n");
    let truncated = dir.file("short.png", b"\x89PN");
    let large = dir.file(
        "large.png",
        &[
            PNG_HEADER,
            &vec![0; MAX_CURSOR_IMAGE_BYTES as usize + 1 - PNG_HEADER.len()],
        ]
        .concat(),
    );
    let folder = dir.0.join("folder.png");
    std::fs::create_dir_all(&folder).unwrap();
    let missing = dir.0.join("missing.png");
    for path in [
        wrong,
        truncated,
        large,
        folder.to_str().unwrap().to_owned(),
        missing.to_str().unwrap().to_owned(),
    ] {
        let image = CursorImage::new(path.clone(), origin()).unwrap();
        let error = image.verify_file().expect_err(&path);
        assert_eq!(error.code, crate::ErrorCode::InvalidArgs);
    }
    let limit = dir.file(
        "limit.png",
        &[
            PNG_HEADER,
            &vec![0; MAX_CURSOR_IMAGE_BYTES as usize - PNG_HEADER.len()],
        ]
        .concat(),
    );
    assert!(
        CursorImage::new(limit, origin())
            .unwrap()
            .verify_file()
            .is_ok()
    );
}

#[test]
fn default_style_omits_the_image_and_old_style_json_still_parses() {
    let json = serde_json::to_string(&CursorOverlayStyle::default()).unwrap();
    assert!(!json.contains("image"));
    let old: CursorOverlayStyle =
        serde_json::from_str(r##"{"fill":"#FFFFFF","rim":"#111318","size":2.0}"##).unwrap();
    assert_eq!(old.image(), None);
    let parsed: CursorOverlayStyle = serde_json::from_str(&format!(
        r#"{{"images":{{"pointer":{{"path":{:?}}}}}}}"#,
        absolute("c.png")
    ))
    .unwrap();
    assert_eq!(parsed.image(), None);
    assert_eq!(
        parsed.pointer_image().map(|image| image.hotspot()),
        Some(&origin())
    );
    let unknown = format!(
        r#"{{"images":{{"arrow":{{"path":{:?},"tint":1}}}}}}"#,
        absolute("c.png")
    );
    assert!(serde_json::from_str::<CursorOverlayStyle>(&unknown).is_err());
    let unknown_slot = format!(
        r#"{{"images":{{"hover":{{"path":{:?}}}}}}}"#,
        absolute("c.png")
    );
    assert!(serde_json::from_str::<CursorOverlayStyle>(&unknown_slot).is_err());
}

#[test]
fn style_validation_rejects_an_invalid_image() {
    for bad in [
        r#"{"images":{"arrow":{"path":"relative.png"}}}"#,
        r#"{"images":{"pointer":{"path":"relative.png"}}}"#,
    ] {
        let style: CursorOverlayStyle = serde_json::from_str(bad).unwrap();
        assert!(style.validated().is_err(), "{bad}");
    }
    let mut style = CursorOverlayStyle::default();
    style.set_image(Some(CursorImage::new(absolute("c.pdf"), origin()).unwrap()));
    style.set_pointer_image(Some(
        CursorImage::new(absolute("hand.png"), Point { x: 7.5, y: 0.0 }).unwrap(),
    ));
    let config = CursorOverlayConfig::enabled(None, 6)
        .unwrap()
        .with_style(style.clone())
        .unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let parsed: CursorOverlayConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.style(), &style);
}

#[test]
fn worst_case_image_present_fits_the_renderer_transport_limit() {
    let base = absolute("x");
    let path = format!(
        "{base}{}.pdf",
        "a".repeat(MAX_CURSOR_IMAGE_PATH_BYTES - base.len() - ".pdf".len())
    );
    let mut style = CursorOverlayStyle::default();
    style.set_fill("#123456".into());
    style.set_size(3.999_999_999_999_999);
    let hotspot = Point {
        x: 511.999_999_999_999,
        y: 511.999_999_999_999,
    };
    style.set_image(Some(
        CursorImage::new(path.clone(), hotspot.clone()).unwrap(),
    ));
    style.set_pointer_image(Some(CursorImage::new(path, hotspot).unwrap()));
    let label = vec!["w".repeat(41); 12].join(" ");
    let config = CursorOverlayConfig::enabled(Some(label), 12).unwrap();
    let far = -12_345.678_9;
    let instruction = CursorOverlayInstruction::new(Point { x: far, y: far }, &config, true)
        .unwrap()
        .with_drag_from(Some(Point { x: far, y: far }))
        .with_target(Some(Rect {
            x: far,
            y: far,
            width: 98_765.432_1,
            height: 98_765.432_1,
        }))
        .with_pointer(true);
    let control = CursorOverlayControl::present_with_style("s".repeat(64), instruction, style)
        .with_agent_id(Some("a".repeat(64)));
    control.validate().unwrap();
    let bytes = serde_json::to_vec(&control).unwrap().len();
    assert!(bytes < 4096, "{bytes} bytes");
}
