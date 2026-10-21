use super::*;

#[test]
fn max_enhance_has_distinct_request_result_and_quote_dimensions() {
    // These pure helper examples include intermediate, non-grid sizes; sending a request
    // first normalizes the input canvas to the model's final generation grid.
    for (width, height, output_width, output_height, quote_width, quote_height) in [
        (512, 512, 1024, 1024, 1024, 1024),
        (1001, 777, 2002, 1554, 1984, 1536),
        (1024, 1024, 1773, 1773, 1760, 1760),
    ] {
        let geometry = max_geometry(ImageSize { width, height }).unwrap();
        assert_eq!(geometry.input_size, ImageSize { width, height });
        assert_eq!(
            geometry.output_size,
            ImageSize {
                width: output_width,
                height: output_height
            }
        );
        assert_eq!(
            geometry.pricing_size,
            ImageSize {
                width: quote_width,
                height: quote_height
            }
        );
    }
}

#[test]
fn invalid_or_extreme_sizes_never_produce_zero_geometry_or_divide_by_zero() {
    for size in [
        ImageSize {
            width: 0,
            height: 100,
        },
        ImageSize {
            width: 100,
            height: 0,
        },
        ImageSize {
            width: 1,
            height: 1,
        },
        ImageSize {
            width: 15,
            height: 512,
        },
        ImageSize {
            width: u32::MAX,
            height: 16,
        },
    ] {
        assert!(max_geometry(size).is_none());
    }
}
