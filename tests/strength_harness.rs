mod common;

use common::{run_strength_harness, DEFAULT_PREVIEW};

#[test]
fn seeded_strength_smoke_test_reaches_basic_survival_floor() {
    let summary = run_strength_harness(0..1, 4, 1, 2);

    assert!(
        summary.failed_seeds().is_empty(),
        "failed seeds: {:?}",
        summary.failed_seeds()
    );
    assert!(
        summary.average_pieces_placed() >= 4.0,
        "average pieces placed too low: {:?}",
        summary
    );
    assert!(
        summary.average_attack_per_piece() >= 0.0,
        "average attack per piece too low: {:?}",
        summary
    );
}

#[test]
#[ignore = "long-running benchmark harness"]
fn hundred_seed_strength_baseline_reports_survival_attack_and_node_metrics() {
    let summary = run_strength_harness(0..100, 200, 1, DEFAULT_PREVIEW);

    assert!(
        summary.failed_seeds().is_empty(),
        "failed seeds: {:?}",
        summary.failed_seeds()
    );
    assert!(
        summary.average_pieces_placed() >= 200.0,
        "average pieces placed too low: {:?}",
        summary
    );
    assert!(
        summary.average_attack_per_piece() >= 0.3,
        "average attack per piece too low: {:?}",
        summary
    );
    assert!(
        summary.average_nodes_per_piece() > 0.0,
        "expected positive node count: {:?}",
        summary
    );
}
