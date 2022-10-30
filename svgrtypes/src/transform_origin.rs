use std::str::FromStr;

use crate::{stream::Stream, Error, Length};

enum Axis {
    X,
    Y,
    Infer,
}

fn convert_transform_origin_token(s: &str) -> Result<(Length, Axis), Error> {
    match s {
        "center" => Ok((Length::new(50.0, crate::LengthUnit::Percent), Axis::Infer)),
        "left" => Ok((Length::new(0.0, crate::LengthUnit::None), Axis::X)),
        "top" => Ok((Length::new(0.0, crate::LengthUnit::None), Axis::Y)),
        "right" => Ok((Length::new(100.0, crate::LengthUnit::Percent), Axis::X)),
        "bottom" => Ok((Length::new(100.0, crate::LengthUnit::Percent), Axis::Y)),
        text => {
            let length = crate::Length::from_str(text)?;
            Ok((length, Axis::Infer))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
/// Defines svg transform-origin attribute value. In svg according to spec origin is always 0;0 until
/// user specifies different. Attribute can contain one value, then it is treated as <x> and <y> is 0
pub struct TransformOrigin {
    /// the x offset for origin
    pub x: crate::length::Length,
    /// the y offset for origin
    pub y: crate::length::Length,
}

impl std::str::FromStr for TransformOrigin {
    type Err = Error;

    fn from_str(text: &str) -> std::result::Result<Self, Error> {
        let mut stream = Stream::from(text);

        let first_part = stream.take_till_space();
        let first_token = convert_transform_origin_token(first_part)?;

        let second_token = if !stream.at_end() {
            stream.skip_spaces();
            let leftover = stream.slice_tail();
            convert_transform_origin_token(leftover)?
        } else {
            (Length::new_number(0.), Axis::Infer)
        };

        // Get the point based on value, make sure that order of axis can vary if keywords submitted
        // E.g. first element can represent x if "left" or "right" is submitted,
        // but if "top" or "bottom" it represents y
        // If both y submitted we ignore invalid axis
        let (x, y) = match (first_token, second_token) {
            ((x, Axis::X), (y, Axis::Y)) => (x, y),
            ((y, Axis::Y), (x, Axis::X)) => (x, y),
            ((x, Axis::X), (_, Axis::X)) => (x, x),
            ((y, Axis::Y), (_, Axis::Y)) => (y, y),
            ((y, Axis::Infer), (x, Axis::X)) => (x, y),
            ((x, Axis::Infer), (y, Axis::Y)) => (x, y),
            ((x, Axis::X), (y, Axis::Infer)) => (x, y),
            ((y, Axis::Y), (x, Axis::Infer)) => (x, y),
            ((x, Axis::Infer), (y, Axis::Infer)) => (x, y),
        };

        Ok(TransformOrigin { x, y })
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use crate::LengthUnit;

    use super::*;
    use std::str::FromStr;

    macro_rules! test {
        ($name:ident, $text:expr, $result:expr) => (
            #[test]
            fn $name() {
                let v = TransformOrigin::from_str($text).unwrap();
                assert_eq!(v, $result);
            }
        )
    }

    test!(parse_1, "center bottom", TransformOrigin {
        x: Length { number: 50., unit: LengthUnit::Percent },
        y: Length { number: 100., unit: LengthUnit::Percent },
    });

    test!(parse_2, "1.4cm 1.4em", TransformOrigin {
        x: Length { number: 1.4, unit: LengthUnit::Cm,},
        y: Length { number: 1.4, unit: LengthUnit::Em,}
    });

    test!(parse_3, "left 10%", TransformOrigin {
        x: Length::new(0.0, LengthUnit::None),
        y: Length { number: 10.0, unit: LengthUnit::Percent, }
    });

    test!(parse_4, "20px", TransformOrigin {
        x: Length { number: 20.0, unit: LengthUnit::Px,},
        y: Length { number: 0.0, unit: LengthUnit::None,}
    });

    test!(parse_5, "center center", TransformOrigin {
        x: Length { number: 50.0, unit: LengthUnit::Percent,},
        y: Length { number: 50.0, unit: LengthUnit::Percent,}
    });

    test!(parse_6, "top top", TransformOrigin {
        x: Length { number: 0.0, unit: LengthUnit::None,},
        y: Length { number: 0.0, unit: LengthUnit::None,}
    });

    test!(parse_7, "bottom bottom", TransformOrigin {
        x: Length { number: 100.0, unit: LengthUnit::Percent,},
        y: Length { number: 100.0, unit: LengthUnit::Percent,}
    });

}
