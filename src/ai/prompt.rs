//! Prompt template for the Claude root-cause classifier.

use crate::ai::ClassifyInput;

const PROMPT_VERSION: u32 = 1;

const TEMPLATE_HEAD: &str = "You are a flaky-test root-cause classifier. Analyze the failure evidence for ONE test and return a single JSON object. Do not include prose outside the JSON.\n\nTAXONOMY (pick exactly one for category):\n  - timing_race  : nondeterministic ordering, sleep-based waits, race conditions\n  - network      : socket errors, DNS, HTTP timeouts, external service unavailability\n  - environment  : env vars, file paths, locale, time zone, OS-specific behavior\n  - ordering     : test depends on other tests' side effects / global state\n  - resource     : OOM, file-descriptor exhaustion, disk full, port collision\n  - unknown      : evidence insufficient\n\nRESPONSE SCHEMA (strict JSON):\n{ \"category\":\"<taxonomy>\", \"confidence\":0.0-1.0, \"summary\":\"<200 chars\", \"evidence\":[\"...\"], \"suggested_fix\":\"<300 chars\", \"needs_more_data\":false }\n\nEXAMPLE\nInput: failures=4/10, messages=[\"OSError: Address already in use\"]\nOutput: {\"category\":\"resource\",\"confidence\":0.92,\"summary\":\"Test binds hardcoded port 8080.\",\"evidence\":[\"Address already in use\"],\"suggested_fix\":\"Bind port 0; read assigned port from socket.\",\"needs_more_data\":false}\n\n";

pub fn build(input: &ClassifyInput) -> (String, String) {
    let mut msgs = input.recent_messages.clone();
    msgs.sort();
    msgs.dedup();
    let log_hash = blake3::hash(input.log_excerpt.as_bytes()).to_hex().to_string();

    let mut prompt = String::from(TEMPLATE_HEAD);
    prompt.push_str("NOW CLASSIFY:\n");
    prompt.push_str(&format!("Test: {}\n", input.test_id.as_str()));
    prompt.push_str(&format!("Framework: {}\n", input.framework));
    prompt.push_str(&format!(
        "Runs: {}, Failures: {}, FlakeProb: {:.2}\n",
        input.runs, input.failures, input.flake_prob
    ));
    prompt.push_str("Recent failure messages (newest first):\n");
    if msgs.is_empty() {
        prompt.push_str("  (none captured)\n");
    } else {
        for m in &msgs {
            let one = m.lines().next().unwrap_or("").chars().take(400).collect::<String>();
            prompt.push_str(&format!("  - {}\n", one));
        }
    }
    prompt.push_str("Log excerpt (truncated):\n");
    prompt.push_str(&input.log_excerpt);
    prompt.push('\n');

    let key_material = format!("v{PROMPT_VERSION}|{}|{}|{}|{}",
        input.test_id.as_str(),
        msgs.join("\u{1e}"),
        log_hash,
        input.framework,
    );
    let cache_key = blake3::hash(key_material.as_bytes()).to_hex().to_string();
    (prompt, cache_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TestId;

    fn input() -> ClassifyInput {
        ClassifyInput {
            test_id: TestId::from_raw("s::x").unwrap(),
            framework: "cargo".into(),
            runs: 10, failures: 3, flake_prob: 0.3,
            recent_messages: vec!["boom".into()],
            log_excerpt: "trace".into(),
        }
    }

    #[test]
    fn cache_key_stable_for_same_input() {
        let (_, k1) = build(&input());
        let (_, k2) = build(&input());
        assert_eq!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_log() {
        let mut a = input();
        let (_, k1) = build(&a);
        a.log_excerpt.push_str(" different");
        let (_, k2) = build(&a);
        assert_ne!(k1, k2);
    }
}
