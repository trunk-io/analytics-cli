use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, FixedOffset};
use clap::Parser;
use fake::Fake;
use quick_junit::{NonSuccessKind, Report, TestCase, TestCaseStatus, TestRerun, TestSuite};
use quick_xml::events::Event;
use quick_xml::{Reader, Writer};
use rand::prelude::*;
use rand::rngs::StdRng;

macro_rules! percentages_parser {
    ($func_name:ident, $num_percentages:literal) => {
        fn $func_name(argument: &str) -> std::result::Result<Vec<u8>, clap::Error> {
            argument
                .split(',')
                .enumerate()
                .try_fold((0_u8, Vec::new()), |mut acc, (i, percentage_str)| {
                    if i >= $num_percentages {
                        return Err(clap::Error::raw(
                            clap::error::ErrorKind::InvalidValue,
                            "More than $num_percentages percentages provided",
                        ));
                    }
                    let percentage = percentage_str
                        .parse::<u8>()
                        .map_err(|e| clap::Error::raw(clap::error::ErrorKind::InvalidValue, e))?;

                    if percentage > 100 {
                        return Err(clap::Error::raw(
                            clap::error::ErrorKind::InvalidValue,
                            format!("Percentage at index {} is greater than 100", i),
                        ));
                    }

                    acc.0 += percentage;

                    if acc.0 > 100 {
                        return Err(clap::Error::raw(
                            clap::error::ErrorKind::InvalidValue,
                            "Sum of percentages are greater than 100",
                        ));
                    }

                    acc.1.push(percentage);

                    Ok(acc)
                })
                .map(|v| v.1)
        }
    };
}

#[derive(Debug, Parser, Clone)]
pub struct Options {
    #[command(flatten, next_help_heading = "Global Options")]
    pub global: GlobalOptions,

    #[command(flatten, next_help_heading = "Report Options")]
    pub report: ReportOptions,

    #[command(flatten, next_help_heading = "Test Suite Options")]
    pub test_suite: TestSuiteOptions,

    #[command(flatten, next_help_heading = "Test Case Options")]
    pub test_case: TestCaseOptions,

    #[command(flatten, next_help_heading = "Test Rerun Options")]
    pub test_rerun: TestRerunOptions,
}

impl Default for Options {
    fn default() -> Self {
        Options::try_parse_from([""]).unwrap()
    }
}

#[test]
fn options_can_be_defaulted_without_panicing() {
    Options::default();
}

#[derive(Debug, Parser, Clone)]
#[group()]
pub struct GlobalOptions {
    /// Seed for all generated data, defaults to randomly generated seed
    #[arg(long)]
    pub seed: Option<u64>,

    /// Timestamp for all data to be based on, defaults to now
    #[arg(long)]
    pub timestamp: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Parser, Clone)]
#[group()]
pub struct ReportOptions {
    /// A list of report names to generate (conflicts with --report-random-count)
    #[arg(long, conflicts_with = "report_random_count")]
    pub report_names: Option<Vec<String>>,

    /// The number of reports with random names to generate (conflicts with --report-names)
    #[arg(long, default_value = "1", conflicts_with = "report_names")]
    pub report_random_count: usize,

    /// Inclusive range of time between report timestamps
    #[arg(long, num_args = 1..=2, value_names = ["DURATION_RANGE_START", "DURATION_RANGE_END"], default_values = ["5m", "1h"])]
    pub report_duration_range: Vec<humantime::Duration>,

    /// Serialize the reports without the top-level `testsuites` element
    #[arg(long)]
    pub do_not_render_testsuites_element: bool,
}

#[derive(Debug, Parser, Clone)]
#[group()]
pub struct TestSuiteOptions {
    /// A list of test suite names to generate (conflicts with --test-suite-random-count)
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "test_suite_random_count"
    )]
    pub test_suite_names: Option<Vec<String>>,

    /// The number of test suites with random names to generate (conflicts with --test-suite-names)
    #[arg(long, conflicts_with = "test_suite_names", default_value = "50")]
    pub test_suite_random_count: usize,

    /// The chance of a system out message being added to the test suite
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_suite_sys_out_percentage: u8,

    /// The chance of a system error message being added to the test suite
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_suite_sys_err_percentage: u8,
}

percentages_parser!(four_percentages_parser, 4);

#[derive(Debug, Parser, Clone)]
#[group()]
pub struct TestCaseOptions {
    /// A list of test case names to generate (conflicts with --test-case-random-count, requires --test-case-classnames)
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "test_case_random_count",
        requires = "test_case_classnames"
    )]
    pub test_case_names: Option<Vec<String>>,

    /// A list of test case classnames to generate (conflicts with --test-case-random-count, requires --test-case-names)
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "test_case_random_count",
        requires = "test_case_names"
    )]
    pub test_case_classnames: Option<Vec<String>>,

    /// The number of test cases with random names to generate (conflicts with --test-suite-names, --test-suite-classnames)
    #[arg(long, conflicts_with_all = ["test_case_names", "test_case_classnames"], default_value = "10")]
    pub test_case_random_count: usize,

    /// The chance of a system out message being added to the test case
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_case_sys_out_percentage: u8,

    /// The chance of a system error message being added to the test case
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_case_sys_err_percentage: u8,

    /// Inclusive range of time between test case timestamps
    #[arg(long, num_args = 1..=2, value_names = ["DURATION_RANGE_START", "DURATION_RANGE_END"], default_values = ["30s", "1m"])]
    pub test_case_duration_range: Vec<humantime::Duration>,

    /// The chance of a test case succeeding, skipping, failing, and erroring (must add up to 100)
    #[arg(long, value_parser = four_percentages_parser, default_value = "25,25,25,25")]
    pub test_case_success_to_skip_to_fail_to_error_percentage: Vec<Vec<u8>>,
}

percentages_parser!(two_percentages_parser, 2);

#[derive(Debug, Parser, Clone)]
#[group()]
pub struct TestRerunOptions {
    /// Inclusive range of the number of reruns of a test that was not skipped
    #[arg(long, num_args = 1..=2, value_names = ["COUNT_RANGE_START", "COUNT_RANGE_END"], default_values = ["0", "2"])]
    pub test_rerun_count_range: Vec<usize>,

    /// The chance of a test rerun failing and erroring (must add up to 100)
    #[arg(long, value_parser = two_percentages_parser, default_value = "50,50")]
    pub test_rerun_fail_to_error_percentage: Vec<Vec<u8>>,

    /// The chance of a system out message being added to the test rerun
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_rerun_sys_out_percentage: u8,

    /// Inclusive range of time between test case timestamps
    #[arg(long, num_args = 1..=2, value_names = ["DURATION_RANGE_START", "DURATION_RANGE_END"], default_values = ["30s", "1m"])]
    pub test_rerun_duration_range: Vec<humantime::Duration>,

    /// The chance of a system error message being added to the test rerun
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100), default_value = "50")]
    pub test_rerun_sys_err_percentage: u8,
}

#[derive(Debug, Clone)]
pub struct JunitMock {
    seed: u64,
    options: Options,

    // state for generating reports
    rng: StdRng,
    timestamp: DateTime<FixedOffset>,
    total_duration: Duration,
}

impl JunitMock {
    pub fn new(options: Options) -> Self {
        let (seed, rng) = JunitMock::rng_from_seed(&options);
        let timestamp = options.global.timestamp.unwrap_or_default();
        Self {
            seed,
            options,
            rng,
            timestamp,
            total_duration: Duration::new(0, 0),
        }
    }

    fn rng_from_seed(options: &Options) -> (u64, StdRng) {
        let seed = options.global.seed.unwrap_or_else(rand::random);
        (seed, StdRng::seed_from_u64(seed))
    }

    pub fn set_options(&mut self, options: Options) {
        let (seed, rng) = JunitMock::rng_from_seed(&options);
        self.seed = seed;
        self.rng = rng;
        self.options = options;
    }

    pub fn get_seed(&self) -> u64 {
        self.seed
    }

    pub fn increment_duration(&mut self, duration: Duration) {
        self.total_duration += duration;
        self.timestamp += duration;
    }

    pub fn generate_reports(&mut self) -> Vec<Report> {
        self.timestamp = self
            .options
            .global
            .timestamp
            .unwrap_or_else(|| chrono::Utc::now().fixed_offset());

        self.options
            .report
            .report_names
            .as_ref()
            .cloned()
            .map(|mut report_names| {
                report_names.shuffle(&mut self.rng);
                report_names
            })
            .unwrap_or_else(|| {
                (0..self.options.report.report_random_count)
                    .map(|_| fake::faker::company::en::Buzzword().fake_with_rng(&mut self.rng))
                    .collect()
            })
            .iter()
            .map(|report_name| {
                let mut report = Report::new(report_name);
                report.set_timestamp(self.timestamp);
                self.total_duration = Duration::new(0, 0);
                report.add_test_suites(self.generate_test_suites());
                report.set_time(self.total_duration);
                let duration =
                    self.fake_duration(self.options.report.report_duration_range.clone());
                self.increment_duration(duration);
                report
            })
            .collect()
    }

    pub fn write_reports_to_file<T: AsRef<Path>, U: AsRef<[Report]>>(
        &self,
        directory: T,
        reports: U,
    ) -> Result<Vec<PathBuf>> {
        reports.as_ref().iter().enumerate().try_fold(
            Vec::new(),
            |mut acc, (i, report)| -> Result<Vec<PathBuf>> {
                let path = directory.as_ref().join(format!("junit-{}.xml", i));
                let mut file = File::create(&path)?;
                if self.options.report.do_not_render_testsuites_element {
                    Self::serialize_without_testsuites(&mut file, report)?
                } else {
                    report.serialize(file)?;
                }
                acc.push(path);
                Ok(acc)
            },
        )
    }

    fn serialize_without_testsuites(file: &mut File, report: &Report) -> Result<()> {
        let serialized_report = report.to_string()?;
        let mut reader = Reader::from_str(&serialized_report);
        reader.config_mut().trim_text(true);
        let mut writer = Writer::new_with_indent(file, b' ', 4);
        loop {
            match reader.read_event()? {
                Event::Start(e) => {
                    if e.name().as_ref() == b"testsuites" {
                        continue;
                    }
                    writer.write_event(Event::Start(e))?;
                }
                Event::End(e) => {
                    if e.name().as_ref() == b"testsuites" {
                        continue;
                    }
                    writer.write_event(Event::End(e))?;
                }
                Event::Eof => break,
                e => writer.write_event(e)?,
            }
        }
        Ok(())
    }

    fn generate_test_suites(&mut self) -> Vec<TestSuite> {
        self.options
            .test_suite
            .test_suite_names
            .as_ref()
            .cloned()
            .map(|mut test_suite_names| {
                test_suite_names.shuffle(&mut self.rng);
                test_suite_names
            })
            .unwrap_or_else(|| {
                (0..self.options.test_suite.test_suite_random_count)
                    .map(|_| fake::faker::company::en::Buzzword().fake_with_rng(&mut self.rng))
                    .collect()
            })
            .iter()
            .map(|test_suite_name| -> TestSuite {
                let mut test_suite = TestSuite::new(test_suite_name);
                test_suite.set_timestamp(self.timestamp);
                let last_duration = self.total_duration;
                test_suite.add_test_cases(self.generate_test_cases());
                test_suite.set_time(self.total_duration - last_duration);
                if self.rand_bool(self.options.test_suite.test_suite_sys_out_percentage) {
                    test_suite.set_system_out(self.fake_paragraphs());
                }
                if self.rand_bool(self.options.test_suite.test_suite_sys_err_percentage) {
                    test_suite.set_system_err(self.fake_paragraphs());
                }
                test_suite
            })
            .collect()
    }

    fn generate_test_cases(&mut self) -> Vec<TestCase> {
        let classnames = self
            .options
            .test_case
            .test_case_classnames
            .as_ref()
            .cloned()
            .map(|mut test_case_classnames| {
                test_case_classnames.shuffle(&mut self.rng);
                test_case_classnames
            })
            .unwrap_or_else(|| {
                (0..self.options.test_case.test_case_random_count)
                    .map(|_| fake::faker::filesystem::en::DirPath().fake_with_rng(&mut self.rng))
                    .collect()
            });

        self.options
            .test_case
            .test_case_names
            .as_ref()
            .cloned()
            .map(|mut test_case_names| {
                test_case_names.shuffle(&mut self.rng);
                test_case_names
            })
            .unwrap_or_else(|| {
                (0..self.options.test_case.test_case_random_count)
                    .map(|_| fake::faker::company::en::Buzzword().fake_with_rng(&mut self.rng))
                    .collect()
            })
            .iter()
            .zip(classnames.iter())
            .map(|(test_case_name, test_case_classname)| -> TestCase {
                let last_duration = self.total_duration;
                let timestamp = self.timestamp;

                let test_case_status = self.generate_test_case_status();
                let is_skipped = matches!(&test_case_status, TestCaseStatus::Skipped { .. });

                let mut test_case = TestCase::new(test_case_name, test_case_status);
                let file: String =
                    fake::faker::filesystem::en::FilePath().fake_with_rng(&mut self.rng);
                test_case.extra.insert("file".into(), file.into());
                test_case.set_classname(format!("{test_case_classname}/{test_case_name}"));
                test_case.set_assertions(self.rng.gen_range(1..10));
                test_case.set_timestamp(timestamp);
                let duration = if is_skipped {
                    Default::default()
                } else {
                    self.fake_duration(self.options.test_case.test_case_duration_range.clone())
                };
                test_case.set_time((self.total_duration + duration) - last_duration);
                self.increment_duration(duration);

                if self.rand_bool(self.options.test_case.test_case_sys_out_percentage) {
                    test_case.set_system_out(self.fake_paragraphs());
                }
                if self.rand_bool(self.options.test_case.test_case_sys_err_percentage) {
                    test_case.set_system_err(self.fake_paragraphs());
                }
                test_case
            })
            .collect()
    }

    fn generate_test_case_status(&mut self) -> TestCaseStatus {
        let rand_percentage = self.rand_percentage();
        let mut total = 0_u8;
        for (i, percentage) in self
            .options
            .test_case
            .test_case_success_to_skip_to_fail_to_error_percentage
            .iter()
            .flatten()
            .enumerate()
        {
            let new_total = total + percentage;
            if (total..=new_total).contains(&rand_percentage) {
                return match i {
                    0 => TestCaseStatus::Success {
                        flaky_runs: self.generate_test_reruns(),
                    },
                    1 => TestCaseStatus::skipped(),
                    2 => TestCaseStatus::NonSuccess {
                        kind: quick_junit::NonSuccessKind::Failure,
                        message: Some(self.fake_paragraphs().into()),
                        ty: None,
                        description: None,
                        reruns: self.generate_test_reruns(),
                    },
                    3 => TestCaseStatus::NonSuccess {
                        kind: quick_junit::NonSuccessKind::Error,
                        message: Some(self.fake_paragraphs().into()),
                        ty: None,
                        description: None,
                        reruns: self.generate_test_reruns(),
                    },
                    _ => unreachable!("only 4 percentages are valid"),
                };
            }
            total = new_total;
        }
        unreachable!("invalid percentage of test case status")
    }

    fn generate_test_reruns(&mut self) -> Vec<TestRerun> {
        let range_start = *self
            .options
            .test_rerun
            .test_rerun_count_range
            .first()
            .expect("test rerun count range must have a start");
        let range_end = *self
            .options
            .test_rerun
            .test_rerun_count_range
            .get(1)
            .expect("test rerun count range must have an end");
        let count = self.rng.gen_range(range_start..=range_end);
        let failure_to_error_threshold = *self
            .options
            .test_rerun
            .test_rerun_fail_to_error_percentage
            .iter()
            .flatten()
            .next()
            .expect("test rerun failure percentage must be set");
        (0..count)
            .map(|_| {
                let mut test_rerun =
                    TestRerun::new(if self.rand_percentage() <= failure_to_error_threshold {
                        NonSuccessKind::Failure
                    } else {
                        NonSuccessKind::Error
                    });

                test_rerun.set_timestamp(self.timestamp);
                let duration =
                    self.fake_duration(self.options.test_rerun.test_rerun_duration_range.clone());
                test_rerun.set_time(duration);
                self.increment_duration(duration);

                test_rerun.set_message(self.fake_sentence());
                if self.rand_bool(self.options.test_rerun.test_rerun_sys_out_percentage) {
                    test_rerun.set_system_out(self.fake_paragraphs());
                }
                if self.rand_bool(self.options.test_rerun.test_rerun_sys_err_percentage) {
                    test_rerun.set_system_err(self.fake_paragraphs());
                }
                test_rerun.set_description(self.fake_sentence());

                test_rerun
            })
            .collect()
    }

    fn fake_sentence(&mut self) -> String {
        let paragraphs: Vec<String> =
            fake::faker::lorem::en::Sentences(1..2).fake_with_rng(&mut self.rng);
        paragraphs.join(" ")
    }

    fn fake_paragraphs(&mut self) -> String {
        let paragraphs: Vec<String> =
            fake::faker::lorem::en::Paragraphs(1..3).fake_with_rng(&mut self.rng);
        paragraphs.join("\n")
    }

    fn fake_duration<T: AsRef<[humantime::Duration]>>(&mut self, range: T) -> Duration {
        let range_start = range
            .as_ref()
            .first()
            .expect("must have start range for duration")
            .as_nanos();
        let range_end = range
            .as_ref()
            .get(1)
            .expect("must have end range for duration")
            .as_nanos();
        let rand_duration_ns = self.rng.gen_range(range_start..=range_end);
        Duration::new(0, rand_duration_ns as u32)
    }

    fn rand_bool<T: Into<f64>>(&mut self, percentage_chance: T) -> bool {
        self.rng.gen_bool(percentage_chance.into() / 100.0)
    }

    fn rand_percentage(&mut self) -> u8 {
        self.rng.gen_range(0..=100)
    }
}
