use codanna::io::{
    ExitCode, ProgressBar, ProgressBarOptions, ProgressBarStyle, Spinner, SpinnerOptions,
};
use std::time::Duration;

#[test]
fn progress_bar_respects_style_and_width() {
    let options = ProgressBarOptions::default()
        .with_style(ProgressBarStyle::VerticalSolid)
        .with_width(4)
        .show_rate(false)
        .show_elapsed(false);

    let bar = ProgressBar::with_options(4, "items", "", "", options);

    for _ in 0..2 {
        bar.inc();
    }

    let rendered = format!("{bar}");
    assert!(
        rendered.starts_with("Progress: [▮▮  ]  50%\n2/4 items"),
        "unexpected rendering: {rendered}"
    );
}

#[test]
fn spinner_reports_failure_with_exit_code() {
    let options = SpinnerOptions::new(Duration::from_millis(40));
    let spinner = Spinner::with_options("Resolving", "retry batches", options);

    spinner.tick();
    spinner.add_extra(2);
    spinner.mark_failure(ExitCode::BlockingError, "network down");

    let rendered = format!("{spinner}");
    assert!(rendered.contains("✗ Resolving failed"));
    assert!(rendered.contains("exit code 2"));
    assert!(rendered.contains("network down"));
    assert_eq!(spinner.current_exit_code(), ExitCode::BlockingError);
}

#[test]
fn spinner_succeeds_and_hides_extra_fields_when_zero() {
    let options = SpinnerOptions::default().with_frame_period(Duration::from_millis(60));
    let spinner = Spinner::with_options("Indexing", "", options);

    for _ in 0..3 {
        spinner.tick();
    }
    spinner.mark_success();

    let rendered = format!("{spinner}");
    assert!(rendered.starts_with("✓ Indexing complete"));
    assert!(!rendered.contains("retry batches"));
}
