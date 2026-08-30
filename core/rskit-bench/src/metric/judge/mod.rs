//! LLM-as-judge metric: a versioned prompt graded by an injected provider, with an untrusted-reply trust boundary and judge provenance.
//!
//! The judge renders a versioned [`JudgePrompt`] over each prediction/reference pair, sends it to an injected [`rskit_llm::Provider`], and parses the reply into a [`JudgeVerdict`] as untrusted structured output. Reply-shape faults are reported as external-service errors, while caller-fault inputs (blank model, unbounded retry policy, over-large prompt) are typed input errors. Each score records the judge identity so runs remain reproducible and comparisons never silently mix judges.

mod metric;
mod parse;
mod prompt;
mod provenance;
mod verdict;

pub use metric::{LlmJudge, llm_judge};
pub use prompt::JudgePrompt;
pub use verdict::JudgeVerdict;

pub(crate) use provenance::judges_from_results;
