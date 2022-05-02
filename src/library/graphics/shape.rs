use std::f64::consts::SQRT_2;

use crate::library::prelude::*;
use crate::library::text::TextNode;

/// Place a node into a sizable and fillable shape.
#[derive(Debug, Hash)]
pub struct ShapeNode<const S: ShapeKind>(pub Option<LayoutNode>);

/// Place a node into a square.
pub type SquareNode = ShapeNode<SQUARE>;

/// Place a node into a rectangle.
pub type RectNode = ShapeNode<RECT>;

/// Place a node into a circle.
pub type CircleNode = ShapeNode<CIRCLE>;

/// Place a node into an ellipse.
pub type EllipseNode = ShapeNode<ELLIPSE>;

#[node]
impl<const S: ShapeKind> ShapeNode<S> {
    /// How to fill the shape.
    pub const FILL: Option<Paint> = None;
    /// How to stroke the shape.
    #[property(resolve, fold)]
    pub const STROKE: Smart<Sides<Option<RawStroke>>> = Smart::Auto;

    /// How much to pad the shape's content.
    #[property(resolve, fold)]
    pub const INSET: Sides<Option<Relative<RawLength>>> = Sides::splat(Relative::zero());

    /// How much to extend the shape's dimensions beyond the allocated space.
    #[property(resolve, fold)]
    pub const OUTSET: Sides<Option<Relative<RawLength>>> = Sides::splat(Relative::zero());

    /// How much to round the shape's corners.
    #[property(resolve, fold)]
    pub const RADIUS: Sides<Option<Relative<RawLength>>> = Sides::splat(Relative::zero());

    fn construct(_: &mut Context, args: &mut Args) -> TypResult<Content> {
        let size = match S {
            SQUARE => args.named::<RawLength>("size")?.map(Relative::from),
            CIRCLE => args.named::<RawLength>("radius")?.map(|r| 2.0 * Relative::from(r)),
            _ => None,
        };

        let width = match size {
            None => args.named("width")?,
            size => size,
        };

        let height = match size {
            None => args.named("height")?,
            size => size,
        };

        Ok(Content::inline(
            Self(args.find()?).pack().sized(Spec::new(width, height)),
        ))
    }

    fn set(args: &mut Args) -> TypResult<StyleMap> {
        let mut styles = StyleMap::new();
        styles.set_opt(Self::FILL, args.named("fill")?);

        if is_round(S) {
            styles.set_opt(
                Self::STROKE,
                args.named::<Smart<Option<RawStroke>>>("stroke")?
                    .map(|some| some.map(Sides::splat)),
            );
        } else {
            styles.set_opt(Self::STROKE, args.named("stroke")?);
        }

        styles.set_opt(Self::INSET, args.named("inset")?);
        styles.set_opt(Self::OUTSET, args.named("outset")?);

        if S != CIRCLE {
            styles.set_opt(Self::RADIUS, args.named("radius")?);
        }

        Ok(styles)
    }
}

impl<const S: ShapeKind> Layout for ShapeNode<S> {
    fn layout(
        &self,
        ctx: &mut Context,
        regions: &Regions,
        styles: StyleChain,
    ) -> TypResult<Vec<Arc<Frame>>> {
        let mut frames;
        if let Some(child) = &self.0 {
            let mut inset = styles.get(Self::INSET);
            if is_round(S) {
                inset = inset.map(|mut side| {
                    side.rel += Ratio::new(0.5 - SQRT_2 / 4.0);
                    side
                });
            }

            // Pad the child.
            let child = child.clone().padded(inset.map(|side| side.map(RawLength::from)));

            let mut pod = Regions::one(regions.first, regions.base, regions.expand);
            frames = child.layout(ctx, &pod, styles)?;

            // Relayout with full expansion into square region to make sure
            // the result is really a square or circle.
            if is_quadratic(S) {
                let length = if regions.expand.x || regions.expand.y {
                    let target = regions.expand.select(regions.first, Size::zero());
                    target.x.max(target.y)
                } else {
                    let size = frames[0].size;
                    let desired = size.x.max(size.y);
                    desired.min(regions.first.x).min(regions.first.y)
                };

                pod.first = Size::splat(length);
                pod.expand = Spec::splat(true);
                frames = child.layout(ctx, &pod, styles)?;
            }
        } else {
            // The default size that a shape takes on if it has no child and
            // enough space.
            let mut size =
                Size::new(Length::pt(45.0), Length::pt(30.0)).min(regions.first);

            if is_quadratic(S) {
                let length = if regions.expand.x || regions.expand.y {
                    let target = regions.expand.select(regions.first, Size::zero());
                    target.x.max(target.y)
                } else {
                    size.x.min(size.y)
                };
                size = Size::splat(length);
            } else {
                size = regions.expand.select(regions.first, size);
            }

            frames = vec![Arc::new(Frame::new(size))];
        }

        let frame = Arc::make_mut(&mut frames[0]);

        // Add fill and/or stroke.
        let fill = styles.get(Self::FILL);
        let mut stroke = match styles.get(Self::STROKE) {
            Smart::Auto if fill.is_none() => Sides::splat(Some(Stroke::default())),
            Smart::Auto => Sides::splat(None),
            Smart::Custom(strokes) => {
                strokes.map(|s| s.map(RawStroke::unwrap_or_default))
            }
        };

        let outset = styles.get(Self::OUTSET);
        let outset = Sides {
            left: outset.left.relative_to(frame.size.x),
            top: outset.top.relative_to(frame.size.y),
            right: outset.right.relative_to(frame.size.x),
            bottom: outset.bottom.relative_to(frame.size.y),
        };

        let size = Spec::new(
            frame.size.x + outset.left + outset.right,
            frame.size.y + outset.top + outset.bottom,
        );

        let radius = styles.get(Self::RADIUS);
        let radius = Sides {
            left: radius.left.relative_to(size.x / 2.0),
            top: radius.top.relative_to(size.y / 2.0),
            right: radius.right.relative_to(size.x / 2.0),
            bottom: radius.bottom.relative_to(size.y / 2.0),
        };

        if fill.is_some() || (stroke.iter().any(Option::is_some) && stroke.is_uniform()) {
            let geometry = if is_round(S) {
                Geometry::Ellipse(size)
            } else {
                Geometry::Rect(size, radius)
            };

            let shape = Shape { geometry, fill, stroke };
            frame.prepend(Point::new(-outset.left, -outset.top), Element::Shape(shape));
        }

        // Apply link if it exists.
        if let Some(url) = styles.get(TextNode::LINK) {
            frame.link(url.clone());
        }

        Ok(frames)
    }
}

/// A category of shape.
pub type ShapeKind = usize;

/// A rectangle with equal side lengths.
const SQUARE: ShapeKind = 0;

/// A quadrilateral with four right angles.
const RECT: ShapeKind = 1;

/// An ellipse with coinciding foci.
const CIRCLE: ShapeKind = 2;

/// A curve around two focal points.
const ELLIPSE: ShapeKind = 3;

/// Whether a shape kind is curvy.
fn is_round(kind: ShapeKind) -> bool {
    matches!(kind, CIRCLE | ELLIPSE)
}

/// Whether a shape kind has equal side length.
fn is_quadratic(kind: ShapeKind) -> bool {
    matches!(kind, SQUARE | CIRCLE)
}
