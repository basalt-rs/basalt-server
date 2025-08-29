use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use bedrock::{packet::Test, Config};
use erudite::{runner::TestRunner, Rules, TestContext};

#[derive(Debug, Copy, Clone, Default)]
pub struct TestData {
    visible: bool,
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
    // language name : test context
    contexts: HashMap<String, ContextExtended>,
}

impl Tester {
    pub fn new(config: &Config) -> Self {
        let contexts = config
            .languages
            .iter()
            .map(|l| {
                let rules = Rules::new()
                    .add_read_only("/usr")
                    .add_read_only("/etc")
                    .add_read_only("/dev")
                    .add_read_only("/bin");

                let mut c = TestContext::builder()
                    .run_command(["bash", "-c", l.run_command()])
                    .run_timeout(config.test_runner.timeout)
                    .trim_output(config.test_runner.trim_output)
                    .test_groups(
                        config
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
                            }),
                    )
                    .rules(rules);

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

        Self { contexts }
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
