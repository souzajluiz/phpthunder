//! The About dialog: the logo's spider walking around the card's edge,
//! drawn as vector geometry so its legs move, over the app name, version,
//! commit, author and repository.

use super::*;
use crate::app::{App, Message};
use crate::tr;
use iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use iced::widget::text::Span;
use iced::widget::{
    button, canvas as canvas_widget, column, container, opaque, rich_text, row, stack,
};
use iced::{
    Center, Color, Element, Fill, Length, Point, Rectangle, Renderer, Theme, Vector, mouse,
};
use std::f32::consts::{FRAC_PI_2, PI};

/// Card size; the spider's track runs inside its border.
const CARD: (f32, f32) = (520.0, 380.0);
/// Distance from the card edge to the line the spider's body follows.
const TRACK_INSET: f32 = 30.0;
/// Corner radius of the track, so the spider turns instead of pivoting.
const TRACK_RADIUS: f32 = 34.0;
/// Walking speed along the track, in px per second.
const SPEED: f32 = 52.0;
/// Full gait cycles (both leg groups stepping) per second.
const STEPS_PER_SECOND: f32 = 1.6;
/// Spider drawing scale: the geometry below is authored about 2x the size
/// it is shown at.
const SPIDER_SCALE: f32 = 0.62;
/// Short hash of the commit this binary was built from (set by `build.rs`).
const GIT_HASH: &str = env!("PHPTHUNDER_GIT_HASH");

/// Femur and tibia lengths of every leg, in authoring units.
const LEG: (f32, f32) = (20.0, 24.0);
/// Right-side legs, front to back: hip point on the cephalothorax, resting
/// angle of the femur (0 = straight out, negative = toward the head) and
/// how far the tibia bends further from it. The left side is the mirror.
const LEGS: [(Point, f32, f32); 4] = [
    (Point::new(4.0, -14.0), -1.05, -0.75),
    (Point::new(5.0, -11.0), -0.40, -0.45),
    (Point::new(5.0, -8.0), 0.25, 0.45),
    (Point::new(4.0, -5.0), 0.90, 0.70),
];

/// Canvas program that draws the spider at its place on the track after
/// `elapsed` seconds. Holds no cache: the whole drawing changes every frame.
struct Spider {
    /// Seconds since the dialog opened.
    elapsed: f32,
}

impl canvas::Program<Message> for Spider {
    type State = ();

    /// Places the spider on the track, turned toward where it walks, and
    /// draws its shadow then its body.
    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (at, heading) = track_point(bounds.size(), self.elapsed * SPEED);
        let phase = self.elapsed * STEPS_PER_SECOND * 2.0 * PI;
        frame.translate(Vector::new(at.x, at.y));
        // The spider is drawn facing up (-y); the track heading is 0 for +x.
        frame.rotate(heading + FRAC_PI_2);
        frame.scale(SPIDER_SCALE);
        frame.with_save(|frame| {
            frame.translate(Vector::new(2.5, 3.5));
            draw_spider(frame, phase, Color::BLACK.scale_alpha(0.35));
        });
        draw_spider(&mut frame, phase, TEXT_BRIGHT);
        vec![frame.into_geometry()]
    }
}

/// Point on the rounded-rectangle track `distance` px clockwise from the top
/// left, and the walking direction there (radians, 0 = +x, clockwise).
/// The track is inset `TRACK_INSET` from a canvas of `size`; distance wraps.
fn track_point(size: iced::Size, distance: f32) -> (Point, f32) {
    let (x0, y0) = (TRACK_INSET, TRACK_INSET);
    let (x1, y1) = (size.width - TRACK_INSET, size.height - TRACK_INSET);
    let r = TRACK_RADIUS
        .min((x1 - x0) / 2.0)
        .min((y1 - y0) / 2.0)
        .max(1.0);
    let horizontal = (x1 - x0 - 2.0 * r).max(0.0);
    let vertical = (y1 - y0 - 2.0 * r).max(0.0);
    let arc = FRAC_PI_2 * r;
    let lap = 2.0 * (horizontal + vertical) + 4.0 * arc;
    let mut left = distance.rem_euclid(lap);
    // Edge k starts at `starts[k]` heading k·90°; the corner after it turns
    // around `corners[k]`.
    let starts = [
        Point::new(x0 + r, y0),
        Point::new(x1, y0 + r),
        Point::new(x1 - r, y1),
        Point::new(x0, y1 - r),
    ];
    let corners = [
        Point::new(x1 - r, y0 + r),
        Point::new(x1 - r, y1 - r),
        Point::new(x0 + r, y1 - r),
        Point::new(x0 + r, y0 + r),
    ];
    for k in 0..4 {
        let heading = k as f32 * FRAC_PI_2;
        let edge = if k % 2 == 0 { horizontal } else { vertical };
        if left < edge {
            let start = starts[k];
            return (
                Point::new(
                    start.x + left * heading.cos(),
                    start.y + left * heading.sin(),
                ),
                heading,
            );
        }
        left -= edge;
        if left < arc {
            let angle = heading - FRAC_PI_2 + left / r;
            let c = corners[k];
            return (
                Point::new(c.x + r * angle.cos(), c.y + r * angle.sin()),
                angle + FRAC_PI_2,
            );
        }
        left -= arc;
    }
    (starts[0], 0.0)
}

/// Draws the spider facing -y around the origin in `color`: eight angular
/// legs stepping in two alternating groups at gait `phase`, the palps, the
/// round cephalothorax and the teardrop abdomen of the logo.
fn draw_spider(frame: &mut Frame, phase: f32, color: Color) {
    let femur = Stroke::default()
        .with_color(color)
        .with_width(3.4)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Miter);
    let tibia = femur.with_width(2.2);
    for (i, &(hip, rest, bend)) in LEGS.iter().enumerate() {
        for side in [1.0_f32, -1.0] {
            // Alternating tetrapod gait: right 0 and 2 step with left 1 and 3.
            let group = (i + usize::from(side < 0.0)) % 2;
            let swing = (phase + group as f32 * PI).sin();
            // Legs reach toward the head while swinging forward, and the
            // knee rises (reads as a longer femur from above).
            let angle = rest - 0.22 * swing;
            let lift = 1.0 + 0.08 * swing.max(0.0);
            let knee = Point::new(
                hip.x + LEG.0 * lift * angle.cos(),
                hip.y + LEG.0 * lift * angle.sin(),
            );
            let foot = Point::new(
                knee.x + LEG.1 * (angle + bend).cos(),
                knee.y + LEG.1 * (angle + bend).sin(),
            );
            let mirror = |p: Point| Point::new(p.x * side, p.y);
            frame.stroke(&Path::line(mirror(hip), mirror(knee)), femur);
            frame.stroke(&Path::line(mirror(knee), mirror(foot)), tibia);
        }
    }
    let palps = Path::new(|p| {
        for side in [1.0_f32, -1.0] {
            p.move_to(Point::new(2.0 * side, -17.0));
            p.quadratic_curve_to(Point::new(6.0 * side, -22.0), Point::new(2.5 * side, -26.0));
        }
    });
    frame.stroke(&palps, tibia);
    frame.fill(&Path::circle(Point::new(0.0, -10.0), 6.5), color);
    frame.fill(&Path::circle(Point::new(0.0, -17.0), 3.6), color);
    let abdomen = Path::new(|p| {
        p.move_to(Point::new(0.0, -3.0));
        p.bezier_curve_to(
            Point::new(13.0, -3.0),
            Point::new(13.0, 14.0),
            Point::new(0.0, 26.0),
        );
        p.bezier_curve_to(
            Point::new(-13.0, 14.0),
            Point::new(-13.0, -3.0),
            Point::new(0.0, -3.0),
        );
    });
    frame.fill(&abdomen, color);
}

impl App {
    /// The About dialog: the walking spider behind the app name, version,
    /// commit, author and repository link. Shown over the backdrop by
    /// `view` while `self.about` is set; empty otherwise.
    pub(crate) fn about_view(&self) -> Element<'_, Message> {
        let Some(about) = &self.about else {
            return iced::widget::space().into();
        };
        let elapsed = about
            .now
            .saturating_duration_since(about.opened)
            .as_secs_f32();
        let spider = canvas_widget(Spider { elapsed }).width(Fill).height(Fill);

        let wordmark = rich_text::<'_, (), _, _, _>([
            Span::new("php").color(TEXT_BRIGHT),
            Span::new("thunder").color(ACCENT),
        ])
        .size(34)
        .font(UI_BOLD);
        let fact = |name: String, value: Element<'static, Message>| {
            row![
                label(name, 12.0, MUTED)
                    .width(Length::Fixed(90.0))
                    .align_x(iced::alignment::Horizontal::Right),
                value,
            ]
            .spacing(14)
            .align_y(Center)
        };
        let repository = env!("CARGO_PKG_REPOSITORY");
        let repo_link = button(label(
            repository.trim_start_matches("https://"),
            13.0,
            ACCENT,
        ))
        .padding([2, 6])
        .style(ghost_button)
        .on_press(Message::OpenRepository);
        let author = env!("CARGO_PKG_AUTHORS").replace(':', ", ");
        let facts = column![
            fact(
                tr!("about-version"),
                label(env!("CARGO_PKG_VERSION"), 13.0, TEXT_BRIGHT)
                    .font(MONO)
                    .into()
            ),
            fact(
                tr!("about-commit"),
                label(GIT_HASH, 13.0, TEXT).font(MONO).into()
            ),
            fact(tr!("about-author"), label(author, 13.0, TEXT).into()),
            fact(tr!("about-repository"), repo_link.into()),
        ]
        .spacing(8);
        let info = column![
            wordmark,
            label(tr!("about-tagline"), 13.0, MUTED),
            container(facts).padding([10, 0]),
            label(
                tr!("about-license", license = env!("CARGO_PKG_LICENSE")),
                11.0,
                GUTTER
            ),
        ]
        .spacing(6)
        .align_x(Center);

        let card = container(stack![spider, container(info).center(Fill)])
            .width(Length::Fixed(CARD.0))
            .height(Length::Fixed(CARD.1))
            .style(popup);
        // Opaque, so a click on the card does not reach the closing backdrop.
        container(opaque(card)).center(Fill).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: iced::Size = iced::Size::new(CARD.0, CARD.1);

    #[test]
    fn track_should_start_at_top_edge_heading_right() {
        let (at, heading) = track_point(SIZE, 0.0);
        assert_eq!(
            (at, heading),
            (Point::new(TRACK_INSET + TRACK_RADIUS, TRACK_INSET), 0.0)
        );
    }

    #[test]
    fn track_should_wrap_after_one_lap() {
        let (w, h) = (CARD.0 - 2.0 * TRACK_INSET, CARD.1 - 2.0 * TRACK_INSET);
        let lap = 2.0 * (w + h) - 8.0 * TRACK_RADIUS + 2.0 * PI * TRACK_RADIUS;
        let (start, _) = track_point(SIZE, 10.0);
        let (again, _) = track_point(SIZE, 10.0 + lap);
        assert!((start.x - again.x).abs() < 0.01 && (start.y - again.y).abs() < 0.01);
    }

    #[test]
    fn track_should_head_down_on_right_edge() {
        let w = CARD.0 - 2.0 * TRACK_INSET - 2.0 * TRACK_RADIUS;
        let (at, heading) = track_point(SIZE, w + FRAC_PI_2 * TRACK_RADIUS + 5.0);
        assert!((heading - FRAC_PI_2).abs() < 1e-5 && (at.x - (CARD.0 - TRACK_INSET)).abs() < 1e-3);
    }
}
