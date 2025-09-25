use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use bedrock::{packet::Test, Config};
use dashmap::DashMap;
use erudite::{runner::TestRunner, Rules, TestContext};
use tokio::sync::oneshot;
use tracing::debug;

use crate::repositories::submissions::SubmissionId;

#[derive(Debug, Copy, Clone, Default)]
pub struct TestData {
    pub visible: bool,
}

impl From<&Test> for TestData {
    fn from(value: &Test) -> Self {
        Self {
            visible: value.visible,
        }
    }
}

pub struct ContextExtended {
    context: Arc<TestContext<usize, TestData>>,
    dest: PathBuf,
}

pub struct Tester {
    // language raw name : test context
    contexts: HashMap<String, ContextExtended>,
    abort_handles: DashMap<SubmissionId, oneshot::Sender<()>>,
}

impl Tester {
    pub fn new(config: &Config) -> Self {
        let start = Instant::now();
        let contexts: HashMap<_, _> = config
            .languages
            .iter()
            .map(|l| {
                let compile_rules = Rules::new()
                    .add_read_only("/tmp")
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");
                let run_rules = Rules::new()
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");

                let groups = config
                    .packet
                    .problems
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        p.languages
                            .as_ref()
                            .is_none_or(|pl| pl.contains(l.raw_name()))
                    })
                    .map(move |(i, p)| {
                        (
                            i,
                            p.tests
                                .iter()
                                .map(|t| (&t.input, &*t.output, TestData::from(t))),
                        )
                    });

                let mut c = TestContext::builder()
                    .run_command(["bash", "-c", l.run_command()])
                    .run_timeout(config.test_runner.timeout)
                    .trim_output(config.test_runner.trim_output)
                    .test_groups(groups)
                    .compile_rules(compile_rules)
                    .run_rules(run_rules);

                if let Some(compile_command) = l.build_command() {
                    c = c.compile_command(["bash", "-c", compile_command])
                };

                (
                    l.raw_name().to_string(),
                    ContextExtended {
                        context: Arc::new(c.build()),
                        dest: PathBuf::from(l.source_file()),
                    },
                )
            })
            .collect();
        debug!(in = ?start.elapsed(), count = contexts.len(), "Generated all test contexts");

        Self {
            contexts,
            abort_handles: Default::default(),
        }
    }

    pub fn add_abort_handle(&self, test: SubmissionId, handle: oneshot::Sender<()>) {
        self.abort_handles.insert(test, handle);
    }

    /// Returns whether the test existed and was successfully aborted
    pub fn abort(&self, test: SubmissionId) -> bool {
        self.abort_handles
            .remove(&test)
            .is_some_and(|(_, x)| x.send(()).is_ok())
    }

    pub fn runner(
        &self,
        language: &str,
        problem_index: usize,
    ) -> Option<(TestRunner<'_, usize, TestData>, &Path)> {
        let ctx = self.contexts.get(language)?;
        Some((
            Arc::clone(&ctx.context).test_runner(&problem_index)?,
            &ctx.dest,
        ))
    }
}
