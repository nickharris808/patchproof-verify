//! Re-check a patchproof elimination certificate, in a language that is not Python.
//!
//! patchproof emits a certificate so a sceptic does not have to trust it. That claim
//! is only worth something if checking it does not mean running our code — and until
//! this crate existed, the only checker was the same Python package that produced the
//! certificate in the first place. A second implementation, written against the
//! format rather than derived from the original, is what turns "verify without
//! trusting us" from a slogan into a property.
//!
//! It is deliberately small enough to audit in one sitting. There is no solver here
//! and there is no arithmetic beyond multiply, add, and compare.
//!
//! # What a certificate claims, and what checking it establishes
//!
//! A certificate is a list of linear inequalities `expr <= 0` with a non-negative
//! integer multiplier each. Scaling every inequality by its multiplier and summing
//! them yields another valid inequality. If every variable cancels and the surviving
//! constant is positive, that sum reads `c <= 0` for some `c > 0` — false — so the
//! original system had no solution. This is the affine form of Farkas' lemma.
//!
//! Checking the arithmetic is necessary and **not sufficient**. A certificate holding
//! the single form `5 <= 0` is arithmetically perfect and says nothing about anyone's
//! patch, because `5 <= 0` is false on its own. So the constraints must also *be* the
//! constraints of the defect class being claimed, checked against a statement of that
//! class held here rather than supplied with the proof. Three outcomes, and the
//! middle one is not a pass:
//!
//! - [`Status::Verified`]   — arithmetic replays *and* the constraints are that class's.
//! - [`Status::Unverified`] — arithmetic replays, but nothing binds it to a claim.
//! - [`Status::Rejected`]   — the arithmetic fails, or the constraints are the wrong ones.

use std::collections::BTreeMap;
use std::fmt;

/// Outcome of checking a certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Arithmetic replays and the constraints match the claimed defect class.
    Verified,
    /// Arithmetic replays, but no claim is bound — so it proves nothing about a patch.
    /// This is **not** a pass.
    Unverified,
    /// The certificate does not check out.
    Rejected,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Verified => "VERIFIED",
            Status::Unverified => "UNVERIFIED",
            Status::Rejected => "REJECTED",
        })
    }
}

impl Status {
    /// Exit code. Only `Verified` succeeds; `Unverified` is a failure, because a
    /// gate that accepted it could be satisfied by a certificate proving nothing.
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Verified => 0,
            _ => 1,
        }
    }
}

/// A linear form `sum(coeff * var) + constant <= 0`.
///
/// Coefficients live in a `BTreeMap` so rendering and comparison are deterministic;
/// a verifier whose output depends on hash ordering is annoying to diff and
/// impossible to test byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearForm {
    pub coeffs: BTreeMap<String, i64>,
    pub constant: i64,
}

impl LinearForm {
    pub fn new() -> Self {
        LinearForm {
            coeffs: BTreeMap::new(),
            constant: 0,
        }
    }

    /// Drop zero coefficients, so `x - x` compares equal to nothing at all.
    fn normalised(mut self) -> Self {
        self.coeffs.retain(|_, v| *v != 0);
        self
    }

    /// Multiply through by a non-negative scalar, checking for overflow.
    ///
    /// Overflow is an error rather than a wrap: a wrapped coefficient could make
    /// variables appear to cancel when they do not, which is precisely the
    /// arithmetic a forged certificate would want.
    pub fn scale(&self, k: i64) -> Result<LinearForm, VerifyError> {
        let mut out = LinearForm::new();
        for (v, c) in &self.coeffs {
            let scaled = c.checked_mul(k).ok_or(VerifyError::Overflow)?;
            out.coeffs.insert(v.clone(), scaled);
        }
        out.constant = self.constant.checked_mul(k).ok_or(VerifyError::Overflow)?;
        Ok(out.normalised())
    }

    pub fn add(&self, other: &LinearForm) -> Result<LinearForm, VerifyError> {
        let mut out = self.clone();
        for (v, c) in &other.coeffs {
            let e = out.coeffs.entry(v.clone()).or_insert(0);
            *e = e.checked_add(*c).ok_or(VerifyError::Overflow)?;
        }
        out.constant = out
            .constant
            .checked_add(other.constant)
            .ok_or(VerifyError::Overflow)?;
        Ok(out.normalised())
    }

    pub fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (v, c) in &self.coeffs {
            let term = match *c {
                1 => v.clone(),
                -1 => format!("- {}", v),
                c if c < 0 => format!("- {}*{}", -c, v),
                c => format!("{}*{}", c, v),
            };
            if parts.is_empty() {
                parts.push(term);
            } else if let Some(stripped) = term.strip_prefix("- ") {
                parts.push(format!("- {}", stripped));
            } else {
                parts.push(format!("+ {}", term));
            }
        }
        if self.constant != 0 || parts.is_empty() {
            let c = self.constant;
            if parts.is_empty() {
                parts.push(c.to_string());
            } else if c < 0 {
                parts.push(format!("- {}", -c));
            } else {
                parts.push(format!("+ {}", c));
            }
        }
        format!("{} <= 0", parts.join(" "))
    }
}

impl Default for LinearForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    Overflow,
    Parse(String),
    Malformed(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::Overflow => write!(
                f,
                "integer overflow while combining the certificate; a wrapped \
                 coefficient could make variables appear to cancel when they do not"
            ),
            VerifyError::Parse(m) => write!(f, "{}", m),
            VerifyError::Malformed(m) => write!(f, "{}", m),
        }
    }
}

/// Parse `a - 2*b + 3 <= 0`.
///
/// Strict by construction: every character must be consumed by a term. A scanner
/// that merely *finds* terms silently ignores what it cannot match, which is how the
/// original Python parser once turned `@@@ <= 0` into the perfectly valid `0 <= 0`.
/// This is the trust boundary — it reads input supplied by whoever wants a claim
/// believed — so unparseable text must never be coerced into a well-formed form.
pub fn parse_form(text: &str) -> Result<LinearForm, VerifyError> {
    let parts: Vec<&str> = text.split("<=").collect();
    if parts.len() != 2 {
        return Err(VerifyError::Parse(format!(
            "unparseable form {:?}: expected exactly one '<=' relation",
            text
        )));
    }
    if parts[1].trim() != "0" {
        return Err(VerifyError::Parse(format!(
            "unparseable form {:?}: right-hand side must be exactly 0, got {:?}",
            text,
            parts[1].trim()
        )));
    }
    let body = parts[0].trim();
    if body.is_empty() {
        return Err(VerifyError::Parse(format!(
            "unparseable form {:?}: empty left-hand side",
            text
        )));
    }

    let mut form = LinearForm::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0usize;
    let mut first = true;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        let mut sign: i64 = 1;
        let mut had_sign = false;
        if chars[i] == '+' || chars[i] == '-' {
            if chars[i] == '-' {
                sign = -1;
            }
            had_sign = true;
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        }
        // Only the leading term may omit its sign; `x y` and `x ++ y` are errors.
        if !had_sign && !first {
            return Err(VerifyError::Parse(format!(
                "unparseable form {:?}: missing '+' or '-' before offset {}",
                text, i
            )));
        }

        // optional integer coefficient followed by '*'
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let digits: String = chars[start..i].iter().collect();
        let mut magnitude: i64 = 1;
        let mut had_coeff = false;
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if !digits.is_empty() && j < chars.len() && chars[j] == '*' {
            magnitude = digits.parse::<i64>().map_err(|_| {
                VerifyError::Parse(format!("coefficient {:?} does not fit in i64", digits))
            })?;
            had_coeff = true;
            i = j + 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        }

        if !digits.is_empty() && !had_coeff {
            // a bare integer term
            let value = digits.parse::<i64>().map_err(|_| {
                VerifyError::Parse(format!("integer {:?} does not fit in i64", digits))
            })?;
            form.constant = form
                .constant
                .checked_add(sign * value)
                .ok_or(VerifyError::Overflow)?;
            first = false;
            continue;
        }

        // an identifier
        let vstart = i;
        if i < chars.len() && (chars[i].is_ascii_alphabetic() || chars[i] == '_') {
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        }
        if vstart == i {
            return Err(VerifyError::Parse(format!(
                "unparseable form {:?}: cannot read a term at offset {}",
                text, vstart
            )));
        }
        let name: String = chars[vstart..i].iter().collect();
        let e = form.coeffs.entry(name).or_insert(0);
        *e = e
            .checked_add(sign * magnitude)
            .ok_or(VerifyError::Overflow)?;
        first = false;
    }

    Ok(form.normalised())
}

/// Combine forms by their multipliers and check the result is a contradiction.
pub fn verify(forms: &[LinearForm], multipliers: &[i64]) -> Result<LinearForm, VerifyError> {
    if forms.len() != multipliers.len() {
        return Err(VerifyError::Malformed(format!(
            "certificate has {} multipliers for {} constraints",
            multipliers.len(),
            forms.len()
        )));
    }
    if multipliers.iter().any(|m| *m < 0) {
        return Err(VerifyError::Malformed(
            "negative multiplier: Farkas requires non-negative multipliers".into(),
        ));
    }
    if !multipliers.iter().any(|m| *m > 0) {
        return Err(VerifyError::Malformed(
            "certificate is empty: every multiplier is zero".into(),
        ));
    }

    let mut combined = LinearForm::new();
    for (f, m) in forms.iter().zip(multipliers.iter()) {
        if *m != 0 {
            combined = combined.add(&f.scale(*m)?)?;
        }
    }
    if !combined.coeffs.is_empty() {
        let surviving: Vec<String> = combined
            .coeffs
            .iter()
            .map(|(v, c)| format!("{}({})", v, c))
            .collect();
        return Err(VerifyError::Malformed(format!(
            "variables did not cancel: {}",
            surviving.join(", ")
        )));
    }
    if combined.constant <= 0 {
        return Err(VerifyError::Malformed(format!(
            "combination yields {} <= 0, which is satisfiable; no contradiction",
            combined.constant
        )));
    }
    Ok(combined)
}

/// The canonical `corrected AND NOT safety` forms per shipped defect class.
///
/// Held by the verifier, not taken from the certificate. That is the entire point:
/// a certificate that supplied its own notion of what class A means could claim
/// anything. These are transcribed from patchproof's `claims.py`, and the shared
/// test vectors in `tests/` check the two agree.
pub fn canonical_forms(class: &str) -> Option<Vec<LinearForm>> {
    let mk = |pairs: &[(&str, i64)], constant: i64| {
        let mut f = LinearForm::new();
        for (v, c) in pairs {
            f.coeffs.insert((*v).to_string(), *c);
        }
        f.constant = constant;
        f.normalised()
    };
    match class {
        "A" => Some(vec![
            mk(&[("index", 1), ("size", -1)], 1),
            mk(&[("size", 1), ("index", -1)], 0),
        ]),
        "B" => Some(vec![
            mk(&[("payload", 1), ("record", -1)], 8),
            mk(&[("record", 1), ("payload", -1)], -7),
        ]),
        "C" => Some(vec![mk(&[("sel", 1)], -19), mk(&[("sel", -1)], 20)]),
        _ => None,
    }
}

/// Check a certificate and, when it names one, that it proves that class.
pub fn replay_bound(cert: &serde_json::Value) -> (Status, String) {
    let entries = match cert.get("constraints").and_then(|c| c.as_array()) {
        Some(a) if !a.is_empty() => a,
        _ => {
            return (
                Status::Rejected,
                "malformed certificate: 'constraints' must be a non-empty list".into(),
            )
        }
    };

    let mut forms = Vec::new();
    let mut mults = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let text = match e.get("form").and_then(|f| f.as_str()) {
            Some(s) => s,
            None => {
                return (
                    Status::Rejected,
                    format!("malformed constraint entry {}: 'form' must be a string", i),
                )
            }
        };
        let form = match parse_form(text) {
            Ok(f) => f,
            Err(err) => {
                return (
                    Status::Rejected,
                    format!("malformed constraint entry {}: {}", i, err),
                )
            }
        };
        // A JSON number that is not an integer must be refused, never truncated:
        // rounding 1.5 to 1 silently checks a *different* certificate.
        let m = match e.get("multiplier") {
            Some(v) if v.is_i64() => v.as_i64().unwrap(),
            Some(v) if v.is_u64() => match i64::try_from(v.as_u64().unwrap()) {
                Ok(m) => m,
                Err(_) => {
                    return (
                        Status::Rejected,
                        format!("malformed constraint entry {}: multiplier too large", i),
                    )
                }
            },
            Some(v) => {
                return (
                    Status::Rejected,
                    format!(
                        "malformed constraint entry {}: multiplier must be an integer, got {}",
                        i, v
                    ),
                )
            }
            None => {
                return (
                    Status::Rejected,
                    format!("malformed constraint entry {}: no multiplier", i),
                )
            }
        };
        forms.push(form);
        mults.push(m);
    }

    let combined = match verify(&forms, &mults) {
        Ok(c) => c,
        Err(err) => return (Status::Rejected, err.to_string()),
    };

    let class = cert
        .get("claim")
        .and_then(|c| c.get("defect_class"))
        .and_then(|c| c.as_str());

    let class = match class {
        Some(c) if !c.is_empty() => c,
        _ => {
            return (
                Status::Unverified,
                format!(
                    "arithmetic replays: the listed inequalities combine to {}, a \
                     contradiction. But the certificate names no defect class, so there \
                     is nothing to check those inequalities against: this does NOT \
                     establish that any patch is complete.",
                    combined.render()
                ),
            )
        }
    };

    let canonical = match canonical_forms(class) {
        Some(c) => c,
        None => {
            return (
                Status::Unverified,
                format!(
                    "arithmetic replays, but defect class {:?} has no canonical linear \
                     form in this verifier (known: A, B, C), so the constraints cannot \
                     be checked against the claim.",
                    class
                ),
            )
        }
    };

    let mut got: Vec<LinearForm> = forms.clone();
    let mut want = canonical;
    let key = |f: &LinearForm| {
        (
            f.coeffs
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
            f.constant,
        )
    };
    got.sort_by_key(key);
    want.sort_by_key(key);
    if got != want {
        return (
            Status::Rejected,
            format!(
                "certificate claims defect class {} but its constraints are not that \
                 class's. A contradiction among other inequalities proves nothing about \
                 this patch.",
                class
            ),
        );
    }

    (
        Status::Verified,
        format!(
            "replayed: combination is {}, a contradiction; constraints match the \
             canonical corrected-AND-NOT-safety forms of class {}",
            combined.render(),
            class
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_form() {
        let f = parse_form("index - size + 1 <= 0").unwrap();
        assert_eq!(f.coeffs.get("index"), Some(&1));
        assert_eq!(f.coeffs.get("size"), Some(&-1));
        assert_eq!(f.constant, 1);
    }

    #[test]
    fn parses_coefficients() {
        let f = parse_form("2*a - 3*b <= 0").unwrap();
        assert_eq!(f.coeffs.get("a"), Some(&2));
        assert_eq!(f.coeffs.get("b"), Some(&-3));
    }

    #[test]
    fn rejects_garbage_rather_than_coercing_it() {
        // Each of these once produced a well-formed LinearForm in the Python parser.
        for bad in [
            "@@@ <= 0",
            "1e9 <= 0",
            "x ++ y <= 0",
            "x <= 0 <= 0",
            "x & y <= 0",
            "x <= 1",
            "x >= 0",
            "<= 0",
            "",
            "x y <= 0",
        ] {
            assert!(parse_form(bad).is_err(), "accepted garbage: {:?}", bad);
        }
    }

    #[test]
    fn overflow_is_an_error_not_a_wrap() {
        let mut f = LinearForm::new();
        f.coeffs.insert("x".into(), i64::MAX);
        assert_eq!(f.scale(2), Err(VerifyError::Overflow));
    }

    #[test]
    fn a_genuine_class_a_certificate_verifies() {
        let cert = serde_json::json!({
            "claim": {"defect_class": "A"},
            "constraints": [
                {"form": "index - size + 1 <= 0", "multiplier": 1},
                {"form": "- index + size <= 0", "multiplier": 1}
            ]
        });
        assert_eq!(replay_bound(&cert).0, Status::Verified);
    }

    #[test]
    fn an_unbound_certificate_is_unverified_not_verified() {
        let cert = serde_json::json!({
            "constraints": [
                {"form": "x + 1 <= 0", "multiplier": 1},
                {"form": "- x <= 0", "multiplier": 1}
            ]
        });
        let (status, msg) = replay_bound(&cert);
        assert_eq!(status, Status::Unverified);
        assert_ne!(status, Status::Verified);
        assert!(msg.contains("does NOT establish"));
        assert_eq!(status.exit_code(), 1);
    }

    #[test]
    fn the_forged_certificate_that_once_verified_is_rejected() {
        let cert = serde_json::json!({
            "constraints": [
                {"form": "@@@ <= 0", "multiplier": 1},
                {"form": "5 <= 0", "multiplier": 1}
            ]
        });
        assert_eq!(replay_bound(&cert).0, Status::Rejected);
    }

    #[test]
    fn a_relabelled_certificate_is_rejected() {
        let cert = serde_json::json!({
            "claim": {"defect_class": "C"},
            "constraints": [
                {"form": "index - size + 1 <= 0", "multiplier": 1},
                {"form": "- index + size <= 0", "multiplier": 1}
            ]
        });
        assert_eq!(replay_bound(&cert).0, Status::Rejected);
    }

    #[test]
    fn float_multipliers_are_rejected_not_truncated() {
        let cert = serde_json::json!({
            "claim": {"defect_class": "A"},
            "constraints": [
                {"form": "index - size + 1 <= 0", "multiplier": 1.5},
                {"form": "- index + size <= 0", "multiplier": 1}
            ]
        });
        let (status, msg) = replay_bound(&cert);
        assert_eq!(status, Status::Rejected);
        assert!(msg.contains("integer"));
    }

    #[test]
    fn negative_multipliers_are_rejected() {
        let cert = serde_json::json!({
            "constraints": [
                {"form": "x + 1 <= 0", "multiplier": -1},
                {"form": "- x <= 0", "multiplier": 1}
            ]
        });
        assert_eq!(replay_bound(&cert).0, Status::Rejected);
    }

    #[test]
    fn every_shipped_class_has_a_verifying_certificate() {
        for class in ["A", "B", "C"] {
            let forms = canonical_forms(class).unwrap();
            let combined = verify(&forms, &vec![1; forms.len()]).unwrap();
            assert!(combined.constant > 0, "class {} does not combine", class);
        }
    }
}
