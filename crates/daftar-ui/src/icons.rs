//! Consistent line icons drawn with egui's painter (no image assets, no emoji).

use egui::{Color32, Rect, Shape, Stroke, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Library,
    Collections,
    Categories,
    Roadmap,
    Bookmark,
    Flashcards,
    Recent,
    Settings,
    Search,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    ChevronUp,
    Plus,
    Minus,
    Import,
    Export,
    Sun,
    Moon,
    Menu,
    SidebarLeft,
    SidebarRight,
    Close,
    Check,
    More,
    Copy,
    Edit,
    Trash,
    Play,
    Flip,
    Shuffle,
    Link,
    FileText,
    Folder,
    FolderOpen,
    Tag,
    Clock,
    ArrowLeft,
    ArrowRight,
    Book,
    Grid,
    List,
    ZoomIn,
    ZoomOut,
    Refresh,
    Save,
    Download,
    Upload,
    Info,
    Warning,
    Error,
    Star,
    Filter,
    Home,
    Keyboard,
    Palette,
    Type,
    Image,
    Pdf,
    Archive,
    Note,
    External,
    Eye,
    Sparkle,
}

impl Icon {
    pub fn draw(&self, painter: &egui::Painter, rect: Rect, color: Color32, width: f32) {
        let s = rect.width().min(rect.height());
        let o = rect.center() - vec2(s * 0.5, s * 0.5);
        let p = |x: f32, y: f32| o + vec2(x * s / 16.0, y * s / 16.0);
        let stroke = Stroke::new(width, color);
        let line = |points: &[(f32, f32)]| {
            painter.add(Shape::line(
                points.iter().map(|(x, y)| p(*x, *y)).collect::<Vec<_>>(),
                stroke,
            ));
        };
        let circle = |cx: f32, cy: f32, r: f32, filled: bool| {
            if filled {
                painter.circle_filled(p(cx, cy), r * s / 16.0, color);
            } else {
                painter.circle_stroke(p(cx, cy), r * s / 16.0, stroke);
            }
        };
        let rrect = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            painter.rect_stroke(
                Rect::from_min_size(p(x, y), vec2(w * s / 16.0, h * s / 16.0)),
                r,
                stroke,
                egui::StrokeKind::Inside,
            );
        };
        match self {
            Icon::Library => {
                line(&[(2.5, 3.0), (2.5, 13.0), (6.5, 13.0), (6.5, 3.0), (2.5, 3.0)]);
                line(&[(8.5, 3.0), (8.5, 13.0), (13.5, 13.0), (13.5, 3.0), (8.5, 3.0)]);
                line(&[(2.5, 5.5), (6.5, 5.5)]);
            }
            Icon::Collections => {
                line(&[(2.0, 5.0), (7.0, 5.0), (8.5, 7.0), (14.0, 7.0), (13.0, 13.0), (2.5, 13.0), (2.0, 5.0)]);
                line(&[(3.5, 2.5), (12.5, 2.5)]);
            }
            Icon::Categories => {
                line(&[(2.5, 2.5), (8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0), (2.5, 2.5)]);
                circle(5.2, 5.2, 1.1, true);
            }
            Icon::Roadmap => {
                line(&[(3.0, 13.0), (3.0, 9.0), (8.0, 9.0), (8.0, 4.0), (13.0, 4.0)]);
                circle(3.0, 13.0, 1.2, true);
                circle(13.0, 4.0, 1.2, true);
                line(&[(10.0, 2.5), (13.0, 4.0), (10.0, 5.5)]);
            }
            Icon::Bookmark => line(&[(4.0, 2.5), (12.0, 2.5), (12.0, 13.5), (8.0, 10.5), (4.0, 13.5), (4.0, 2.5)]),
            Icon::Flashcards => {
                rrect(2.5, 4.0, 9.0, 9.5, 1.5);
                line(&[(5.0, 2.5), (13.5, 2.5), (13.5, 11.0)]);
            }
            Icon::Recent => {
                circle(8.0, 8.0, 6.0, false);
                line(&[(8.0, 4.5), (8.0, 8.2), (11.0, 9.8)]);
            }
            Icon::Settings => {
                circle(8.0, 8.0, 2.6, false);
                for angle in [0.0f32, 60.0, 120.0, 180.0, 240.0, 300.0] {
                    let rad = angle.to_radians();
                    let (sx, sy) = rad.sin_cos();
                    line(&[(8.0 + sx * 4.6, 8.0 + sy * 4.6), (8.0 + sx * 6.2, 8.0 + sy * 6.2)]);
                }
            }
            Icon::Search => {
                circle(7.0, 7.0, 4.4, false);
                line(&[(10.5, 10.5), (14.0, 14.0)]);
            }
            Icon::ChevronLeft => line(&[(10.0, 3.5), (5.5, 8.0), (10.0, 12.5)]),
            Icon::ChevronRight => line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]),
            Icon::ChevronDown => line(&[(3.5, 6.0), (8.0, 10.5), (12.5, 6.0)]),
            Icon::ChevronUp => line(&[(3.5, 10.0), (8.0, 5.5), (12.5, 10.0)]),
            Icon::Plus => {
                line(&[(8.0, 3.0), (8.0, 13.0)]);
                line(&[(3.0, 8.0), (13.0, 8.0)]);
            }
            Icon::Minus => line(&[(3.0, 8.0), (13.0, 8.0)]),
            Icon::Import => {
                line(&[(8.0, 2.5), (8.0, 10.0)]);
                line(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)]);
                line(&[(3.0, 13.0), (13.0, 13.0)]);
            }
            Icon::Export => {
                line(&[(8.0, 10.0), (8.0, 2.5)]);
                line(&[(4.5, 6.0), (8.0, 2.5), (11.5, 6.0)]);
                line(&[(3.0, 13.0), (13.0, 13.0)]);
            }
            Icon::Sun => {
                circle(8.0, 8.0, 3.0, false);
                for (x1, y1, x2, y2) in [
                    (8.0, 1.5, 8.0, 3.2),
                    (8.0, 12.8, 8.0, 14.5),
                    (1.5, 8.0, 3.2, 8.0),
                    (12.8, 8.0, 14.5, 8.0),
                    (3.4, 3.4, 4.6, 4.6),
                    (11.4, 11.4, 12.6, 12.6),
                    (12.6, 3.4, 11.4, 4.6),
                    (4.6, 11.4, 3.4, 12.6),
                ] {
                    line(&[(x1, y1), (x2, y2)]);
                }
            }
            Icon::Moon => {
                let pts: Vec<(f32, f32)> = (0..24)
                    .map(|i| {
                        let a = (i as f32 / 23.0) * std::f32::consts::PI * 1.55 - 0.5;
                        (8.0 + a.cos() * 5.6, 8.0 - a.sin() * 5.6)
                    })
                    .collect();
                line(&pts);
                line(&[(9.6, 3.0), (8.5, 5.0), (10.5, 7.5), (8.0, 10.0), (10.5, 12.5)]);
            }
            Icon::Menu => {
                line(&[(3.0, 5.0), (13.0, 5.0)]);
                line(&[(3.0, 8.0), (13.0, 8.0)]);
                line(&[(3.0, 11.0), (13.0, 11.0)]);
            }
            Icon::SidebarLeft => {
                rrect(2.0, 3.0, 12.0, 10.0, 1.5);
                line(&[(6.0, 3.0), (6.0, 13.0)]);
            }
            Icon::SidebarRight => {
                rrect(2.0, 3.0, 12.0, 10.0, 1.5);
                line(&[(10.0, 3.0), (10.0, 13.0)]);
            }
            Icon::Close => {
                line(&[(4.0, 4.0), (12.0, 12.0)]);
                line(&[(12.0, 4.0), (4.0, 12.0)]);
            }
            Icon::Check => line(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.0)]),
            Icon::More => {
                circle(4.0, 8.0, 0.9, true);
                circle(8.0, 8.0, 0.9, true);
                circle(12.0, 8.0, 0.9, true);
            }
            Icon::Copy => {
                rrect(5.0, 5.0, 8.0, 8.5, 1.0);
                line(&[(3.0, 11.0), (3.0, 2.5), (11.5, 2.5)]);
            }
            Icon::Edit => {
                line(&[(3.0, 13.0), (3.5, 10.0), (10.5, 3.0), (13.0, 5.5), (6.0, 12.5), (3.0, 13.0)]);
            }
            Icon::Trash => {
                line(&[(3.0, 4.5), (13.0, 4.5)]);
                line(&[(6.0, 4.5), (6.0, 2.5), (10.0, 2.5), (10.0, 4.5)]);
                line(&[(4.5, 4.5), (5.5, 13.5), (10.5, 13.5), (11.5, 4.5)]);
            }
            Icon::Play => line(&[(5.5, 3.5), (12.5, 8.0), (5.5, 12.5), (5.5, 3.5)]),
            Icon::Flip => {
                rrect(2.5, 4.0, 9.0, 8.0, 1.5);
                line(&[(14.0, 4.0), (14.0, 12.0)]);
                line(&[(11.0, 6.0), (13.0, 8.0), (11.0, 10.0)]);
            }
            Icon::Shuffle => {
                line(&[(2.5, 5.0), (5.5, 5.0), (10.5, 11.0), (13.5, 11.0)]);
                line(&[(2.5, 11.0), (5.5, 11.0), (10.5, 5.0), (13.5, 5.0)]);
                line(&[(11.5, 3.0), (13.5, 5.0), (11.5, 7.0)]);
                line(&[(11.5, 9.0), (13.5, 11.0), (11.5, 13.0)]);
            }
            Icon::Link => {
                line(&[(6.5, 9.5), (9.5, 6.5)]);
                line(&[(4.5, 8.0), (3.0, 9.5), (3.0, 12.0), (4.0, 13.0), (6.5, 13.0), (8.0, 11.5)]);
                line(&[(11.5, 8.0), (13.0, 6.5), (13.0, 4.0), (12.0, 3.0), (9.5, 3.0), (8.0, 4.5)]);
            }
            Icon::FileText => {
                line(&[(3.5, 2.0), (9.5, 2.0), (12.5, 5.0), (12.5, 14.0), (3.5, 14.0), (3.5, 2.0)]);
                line(&[(9.5, 2.0), (9.5, 5.0), (12.5, 5.0)]);
                line(&[(5.5, 8.0), (10.5, 8.0)]);
                line(&[(5.5, 10.5), (10.5, 10.5)]);
            }
            Icon::Folder | Icon::FolderOpen => {
                line(&[(2.0, 4.0), (6.5, 4.0), (7.5, 5.5), (14.0, 5.5), (14.0, 13.0), (2.0, 13.0), (2.0, 4.0)]);
                if *self == Icon::FolderOpen {
                    line(&[(2.0, 13.0), (4.0, 8.0), (14.5, 8.0), (12.5, 13.0)]);
                }
            }
            Icon::Tag => {
                line(&[(2.5, 2.5), (8.0, 2.5), (13.5, 8.0), (8.0, 13.5), (2.5, 8.0), (2.5, 2.5)]);
                circle(5.3, 5.3, 1.0, true);
            }
            Icon::Clock => {
                circle(8.0, 8.0, 6.0, false);
                line(&[(8.0, 4.5), (8.0, 8.0), (10.8, 9.6)]);
            }
            Icon::ArrowLeft => {
                line(&[(13.0, 8.0), (3.0, 8.0)]);
                line(&[(7.0, 4.0), (3.0, 8.0), (7.0, 12.0)]);
            }
            Icon::ArrowRight => {
                line(&[(3.0, 8.0), (13.0, 8.0)]);
                line(&[(9.0, 4.0), (13.0, 8.0), (9.0, 12.0)]);
            }
            Icon::Book => {
                line(&[(2.5, 3.0), (8.0, 4.0), (8.0, 13.5)]);
                line(&[(13.5, 3.0), (8.0, 4.0)]);
                line(&[(2.5, 3.0), (2.5, 12.5), (8.0, 13.5), (13.5, 12.5), (13.5, 3.0)]);
            }
            Icon::Grid => {
                for (x, y) in [(2.5, 2.5), (9.0, 2.5), (2.5, 9.0), (9.0, 9.0)] {
                    rrect(x, y, 4.5, 4.5, 1.0);
                }
            }
            Icon::List => {
                for y in [4.0, 8.0, 12.0] {
                    circle(3.0, y, 0.9, true);
                    line(&[(6.0, y), (13.5, y)]);
                }
            }
            Icon::ZoomIn => {
                circle(7.0, 7.0, 4.4, false);
                line(&[(10.5, 10.5), (14.0, 14.0)]);
                line(&[(5.0, 7.0), (9.0, 7.0)]);
                line(&[(7.0, 5.0), (7.0, 9.0)]);
            }
            Icon::ZoomOut => {
                circle(7.0, 7.0, 4.4, false);
                line(&[(10.5, 10.5), (14.0, 14.0)]);
                line(&[(5.0, 7.0), (9.0, 7.0)]);
            }
            Icon::Refresh => {
                let pts: Vec<(f32, f32)> = (0..18)
                    .map(|i| {
                        let a = (i as f32 / 17.0) * std::f32::consts::TAU * 0.82;
                        (8.0 + (a + 0.8).cos() * 5.3, 8.0 + (a + 0.8).sin() * 5.3)
                    })
                    .collect();
                line(&pts);
                line(&[(11.5, 2.0), (13.2, 4.6), (10.2, 5.2)]);
            }
            Icon::Save => {
                line(&[(2.5, 3.0), (11.0, 3.0), (13.5, 5.5), (13.5, 13.0), (2.5, 13.0), (2.5, 3.0)]);
                line(&[(4.5, 3.0), (4.5, 6.0), (10.0, 6.0), (10.0, 3.0)]);
                rrect(5.0, 9.0, 6.0, 4.0, 0.5);
            }
            Icon::Download => {
                line(&[(8.0, 2.5), (8.0, 10.0)]);
                line(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)]);
                line(&[(2.5, 13.5), (13.5, 13.5)]);
            }
            Icon::Upload => {
                line(&[(8.0, 10.0), (8.0, 2.5)]);
                line(&[(4.5, 6.0), (8.0, 2.5), (11.5, 6.0)]);
                line(&[(2.5, 13.5), (13.5, 13.5)]);
            }
            Icon::Info => {
                circle(8.0, 8.0, 6.0, false);
                circle(8.0, 5.0, 0.8, true);
                line(&[(8.0, 7.5), (8.0, 11.0)]);
            }
            Icon::Warning => {
                line(&[(8.0, 2.0), (14.0, 13.5), (2.0, 13.5), (8.0, 2.0)]);
                line(&[(8.0, 6.0), (8.0, 9.5)]);
                circle(8.0, 11.3, 0.8, true);
            }
            Icon::Error => {
                circle(8.0, 8.0, 6.0, false);
                line(&[(5.5, 5.5), (10.5, 10.5)]);
                line(&[(10.5, 5.5), (5.5, 10.5)]);
            }
            Icon::Star => {
                let pts = vec![
                    (8.0, 2.0),
                    (9.8, 6.0),
                    (14.0, 6.5),
                    (10.8, 9.3),
                    (11.8, 13.5),
                    (8.0, 11.2),
                    (4.2, 13.5),
                    (5.2, 9.3),
                    (2.0, 6.5),
                    (6.2, 6.0),
                    (8.0, 2.0),
                ];
                line(&pts);
            }
            Icon::Filter => line(&[(2.5, 4.0), (13.5, 4.0), (9.5, 8.5), (9.5, 13.0), (6.5, 11.0), (6.5, 8.5), (2.5, 4.0)]),
            Icon::Home => {
                line(&[(2.5, 7.5), (8.0, 2.5), (13.5, 7.5)]);
                line(&[(4.0, 6.5), (4.0, 13.5), (12.0, 13.5), (12.0, 6.5)]);
                line(&[(6.5, 13.5), (6.5, 9.5), (9.5, 9.5), (9.5, 13.5)]);
            }
            Icon::Keyboard => {
                rrect(1.5, 4.0, 13.0, 8.0, 1.5);
                for x in [3.5f32, 6.0, 8.5, 11.0] {
                    circle(x, 6.6, 0.5, true);
                }
                line(&[(4.5, 9.4), (11.5, 9.4)]);
            }
            Icon::Palette => {
                circle(8.0, 8.0, 6.0, false);
                circle(5.8, 5.8, 1.0, true);
                circle(10.2, 5.8, 1.0, true);
                circle(5.8, 10.2, 1.0, true);
                line(&[(8.0, 12.5), (8.0, 14.0)]);
            }
            Icon::Type => {
                line(&[(3.0, 4.0), (13.0, 4.0)]);
                line(&[(8.0, 4.0), (8.0, 13.0)]);
                line(&[(6.0, 13.0), (10.0, 13.0)]);
            }
            Icon::Image => {
                rrect(2.0, 3.0, 12.0, 10.0, 1.5);
                circle(5.5, 6.0, 1.2, true);
                line(&[(3.0, 12.0), (7.0, 8.0), (9.5, 10.5), (11.5, 8.5), (14.0, 12.0)]);
            }
            Icon::Pdf => {
                line(&[(3.5, 2.0), (9.5, 2.0), (12.5, 5.0), (12.5, 14.0), (3.5, 14.0), (3.5, 2.0)]);
                line(&[(9.5, 2.0), (9.5, 5.0), (12.5, 5.0)]);
                line(&[(5.5, 9.0), (10.5, 9.0)]);
                line(&[(5.5, 11.5), (9.0, 11.5)]);
            }
            Icon::Archive => {
                rrect(2.0, 3.0, 12.0, 10.0, 1.5);
                line(&[(2.0, 6.0), (14.0, 6.0)]);
                line(&[(7.0, 6.0), (7.0, 9.0), (9.0, 9.0), (9.0, 6.0)]);
            }
            Icon::Note => {
                line(&[(3.0, 2.5), (13.0, 2.5), (13.0, 10.0), (9.0, 13.5), (3.0, 13.5), (3.0, 2.5)]);
                line(&[(13.0, 10.0), (9.0, 10.0), (9.0, 13.5)]);
                line(&[(5.5, 6.0), (10.5, 6.0)]);
            }
            Icon::External => {
                line(&[(9.0, 2.5), (13.5, 2.5), (13.5, 7.0)]);
                line(&[(13.0, 3.0), (7.5, 8.5)]);
                line(&[(11.0, 9.5), (11.0, 13.5), (2.5, 13.5), (2.5, 5.0), (6.5, 5.0)]);
            }
            Icon::Eye => {
                line(&[(1.5, 8.0), (4.5, 4.5), (8.0, 3.8), (11.5, 4.5), (14.5, 8.0), (11.5, 11.5), (8.0, 12.2), (4.5, 11.5), (1.5, 8.0)]);
                circle(8.0, 8.0, 2.0, false);
            }
            Icon::Sparkle => {
                line(&[(8.0, 1.5), (9.0, 6.5), (14.0, 8.0), (9.0, 9.5), (8.0, 14.5), (7.0, 9.5), (2.0, 8.0), (7.0, 6.5), (8.0, 1.5)]);
            }
        }
    }
}

/// Paint an icon centered in a rect.
pub fn draw_icon(painter: &egui::Painter, icon: Icon, rect: Rect, color: Color32) {
    icon.draw(painter, rect, color, 1.35);
}

/// Decode the bundled application mark into an egui texture.
pub fn app_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    const PNG: &[u8] = include_bytes!("../../../assets/icons/daftar.png");
    let image = image::load_from_memory(PNG).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("daftar-app-logo", pixels, egui::TextureOptions::LINEAR))
}
