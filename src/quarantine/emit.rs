//! Emit the framework-specific skip annotation for a test.

use crate::domain::{Framework, TestId};

/// Returns a copy-paste snippet that tells the test framework to skip `test_id`.
pub fn emit_skip(framework: Framework, test_id: &TestId, reason: &str) -> String {
    let id = test_id.as_str();
    match framework {
        Framework::Jest | Framework::Vitest => format!(
            "// Quarantined by flaketide: {reason}\n\
             test.skip('{}', () => {{ /* flaketide-quarantined */ }});",
            escape_js(id)
        ),
        Framework::PytestJunit | Framework::PytestJson => format!(
            "# Quarantined by flaketide: {reason}\n\
             @pytest.mark.skip(reason=\"flaketide-quarantined: {reason}\")\n\
             def test_quarantined_{}(): pass",
            slug(id)
        ),
        Framework::GoTest => format!(
            "// Quarantined by flaketide: {reason}\n\
             func TestQuarantined_{}(t *testing.T) {{ t.Skip(\"flaketide-quarantined: {reason}\") }}",
            slug(id)
        ),
        Framework::Cargo | Framework::Nextest => format!(
            "// Quarantined by flaketide: {reason}\n\
             #[ignore = \"flaketide-quarantined: {reason}\"]\n\
             #[test]\n\
             fn quarantined_{}() {{}}",
            slug(id)
        ),
        Framework::JunitXml => format!(
            "// Quarantined by flaketide: {reason}\n\
             @Disabled(\"flaketide-quarantined: {reason}\")\n\
             @Test\n\
             void quarantined_{}() {{}}",
            slug(id)
        ),
    }
}

fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TestId;

    #[test]
    fn slugifies() {
        assert_eq!(slug("foo::bar baz"), "foo__bar_baz");
        assert_eq!(slug("__leading__trailing__"), "leading__trailing");
    }

    #[test]
    fn emits_for_every_framework() {
        let id = TestId::new("suite", "name").unwrap();
        for fw in [
            Framework::Jest, Framework::Vitest, Framework::PytestJson,
            Framework::PytestJunit, Framework::GoTest, Framework::Cargo,
            Framework::Nextest, Framework::JunitXml,
        ] {
            let s = emit_skip(fw, &id, "demo");
            assert!(s.contains("quarantine") || s.contains("Quarantined") || s.contains("skip") || s.contains("Skip"));
        }
    }
}
