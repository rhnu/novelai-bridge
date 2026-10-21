use super::*;

#[test]
fn registry_identity_is_unique_and_serialization_uses_it() {
    let identifiers: std::collections::BTreeSet<_> = Model::ALL.iter().map(Model::as_str).collect();
    assert_eq!(identifiers.len(), Model::ALL.len());
    for model in Model::ALL {
        assert_eq!(model.descriptor().identity().model, model);
        let serialized = serde_json::to_string(&model).unwrap();
        assert_eq!(
            serde_json::from_str::<String>(&serialized).unwrap(),
            model.as_str()
        );
        assert_eq!(serde_json::from_str::<Model>(&serialized).unwrap(), model);
    }
    assert!(serde_json::from_str::<Model>("\"unknown-model\"").is_err());
}

#[test]
fn registered_factory_values_satisfy_their_constraints() {
    for model in Model::ALL {
        let descriptor = model.descriptor();
        let defaults = descriptor.defaults();
        let constraints = descriptor.constraints();
        assert!((constraints.steps.min..=constraints.steps.max).contains(&defaults.sampling.steps));
        assert!((constraints.scale.min..=constraints.scale.max).contains(&defaults.sampling.scale));
        assert!((constraints.samples.min..=constraints.samples.max).contains(&defaults.n_samples));
        assert!(defaults.size.pixels() <= constraints.canvas.max_pixels);
        for dimension in [defaults.size.width, defaults.size.height] {
            assert!(
                (constraints.canvas.dimension.min..=constraints.canvas.dimension.max)
                    .contains(&dimension)
            );
            assert!(dimension.is_multiple_of(constraints.canvas.multiple));
        }
    }
}

#[test]
fn geometry_and_schedule_rules_are_independent_of_wire_shape() {
    assert!(
        (CharacterPositionProfile::Grid5x5.resolve_coordinate(0.49) - 0.5).abs() < f32::EPSILON
    );
    assert!(
        (CharacterPositionProfile::Freeform.resolve_coordinate(0.681) - 0.681).abs() < f32::EPSILON
    );
    assert_eq!(
        NoiseScheduleSupport::ExplicitOnly.resolve(crate::NoiseSchedule::Native),
        crate::NoiseSchedule::Karras
    );
    assert_eq!(
        NoiseScheduleSupport::NativeAndExplicit.resolve(crate::NoiseSchedule::Native),
        crate::NoiseSchedule::Native
    );
    assert_eq!(
        NoiseScheduleSupport::ExplicitOnly.resolve(crate::NoiseSchedule::Exponential),
        crate::NoiseSchedule::Exponential
    );
}
