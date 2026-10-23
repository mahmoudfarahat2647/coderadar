//! Deterministic fix templates.
//!
//! A template turns the *text* of a file into a list of edits. It never touches
//! the filesystem: the result is always reviewed as a unified diff before it is
//! applied. Templates are deliberately conservative - when the pattern does not
//! match exactly, no edit is produced at all.
//!
//! LLM generated fixes live in [`crate::tier2`] (Phase 8) and are never a
//! template: they return a whole-file draft that must pass the same
//! review-before-apply path.

use std::sync::Arc;

use coderadar_domain::{apply_edits, DomainError, LineEdit, PatchProposal};

use crate::ports::{SourceFile, SourceFileReader};

/// A deterministic, review-before-apply remediation for one rule.
pub struct FixTemplate {
    /// The rule this template remediates.
    pub rule_id: &'static str,
    /// Short human readable title.
    pub title: &'static str,
    /// Why the transformation is safe.
    pub explanation: &'static str,
    /// The transformation itself; `None` when the pattern does not match.
    pub apply: fn(&str) -> Option<Vec<LineEdit>>,
}

const JWT_TEMPLATE: FixTemplate = FixTemplate {
    rule_id: "SEC-JWT-003",
    title: "Verify the JWT instead of decoding it",
    explanation:
        "jwt.decode() reads the payload without checking the signature, so an attacker can forge \
         a token. jwt.verify() validates the signature and the algorithm.",
    apply: fix_jwt_decode,
};

const SQL_TEMPLATE: FixTemplate = FixTemplate {
    rule_id: "SEC-INJ-001",
    title: "Parameterise the SQL statement",
    explanation:
        "Concatenating user input into a SQL statement allows injection. The value is bound as \
         a query parameter instead.",
    apply: fix_sql_injection,
};

const SECRET_TEMPLATE: FixTemplate = FixTemplate {
    rule_id: "SEC-TOK-002",
    title: "Move the credential into an environment variable",
    explanation: "Hardcoded credentials leak with the source code. The literal is replaced by an \
         environment variable reference.",
    apply: fix_hardcoded_secret,
};

const TLS_TEMPLATE: FixTemplate = FixTemplate {
    rule_id: "SEC-TLS-005",
    title: "Harden the TLS configuration",
    explanation:
        "Certificate validation must stay enabled and only modern TLS versions are accepted.",
    apply: fix_tls,
};

/// Every deterministic template shipped with the studio.
pub const ALL_TEMPLATES: &[FixTemplate] =
    &[JWT_TEMPLATE, SQL_TEMPLATE, SECRET_TEMPLATE, TLS_TEMPLATE];

/// Look up the template of a rule.
pub fn template_for(rule_id: &str) -> Option<&'static FixTemplate> {
    ALL_TEMPLATES
        .iter()
        .find(|template| template.rule_id == rule_id)
}

/// The rule ids that have a deterministic template.
pub fn templated_rule_ids() -> Vec<&'static str> {
    ALL_TEMPLATES
        .iter()
        .map(|template| template.rule_id)
        .collect()
}

/// Use case: turn a finding into a reviewable [`PatchProposal`].
///
/// The use case owns no I/O: it asks a [`SourceFileReader`] for the file, asks
/// the template for the edits, and assembles the proposal.
pub struct PreparePatch {
    reader: Arc<dyn SourceFileReader>,
}

impl PreparePatch {
    /// Create the use case on top of a source reader.
    pub fn new(reader: Arc<dyn SourceFileReader>) -> Self {
        Self { reader }
    }

    /// Build the proposal for `rule_id` in `relative_path` inside `root`.
    ///
    /// # Errors
    /// Returns [`DomainError::Invariant`] when the rule has no deterministic
    /// template or when the template does not match the file, and
    /// [`DomainError::NotFound`] / [`DomainError::Io`] when the file cannot be
    /// read.
    pub async fn prepare(
        &self,
        root: &str,
        relative_path: &str,
        rule_id: &str,
    ) -> Result<PatchProposal, DomainError> {
        let source = self.reader.read_source(root, relative_path).await?;
        self.prepare_from_source(&source, relative_path, rule_id)
            .await
    }

    /// Build the proposal from an already read [`SourceFile`].
    ///
    /// Reading the file once and reusing it keeps the diff and the fingerprint
    /// consistent with each other.
    ///
    /// # Errors
    /// Returns [`DomainError::Invariant`] when the rule has no deterministic
    /// template or when the template does not match the file.
    pub async fn prepare_from_source(
        &self,
        source: &SourceFile,
        relative_path: &str,
        rule_id: &str,
    ) -> Result<PatchProposal, DomainError> {
        let template = template_for(rule_id).ok_or_else(|| {
            DomainError::invariant(format!(
                "no deterministic fix template for rule '{rule_id}'"
            ))
        })?;

        let edits = (template.apply)(&source.content).ok_or_else(|| {
            DomainError::invariant(format!(
                "no deterministic fix matched '{rule_id}' in '{relative_path}'"
            ))
        })?;

        let new_content = apply_edits(&source.content, &edits)?;
        Ok(PatchProposal::new(
            relative_path,
            rule_id,
            source.sha256.clone(),
            source.line_count,
            edits,
            new_content,
            template.explanation,
        ))
    }
}

// ---------------------------------------------------------------------------
// SEC-JWT-003: jwt.decode -> jwt.verify
// ---------------------------------------------------------------------------

fn fix_jwt_decode(content: &str) -> Option<Vec<LineEdit>> {
    let mut edits = Vec::new();
    let mut cursor = 0usize;

    while let Some(found) = content[cursor..].find("jwt.decode(") {
        let call_start = cursor + found;
        let open = call_start + "jwt.decode".len();
        let close = find_matching_paren(content, open)?;
        let args = content[open + 1..close].trim().to_string();
        let parts = split_top_level_args(&args);

        let new_args = match parts.len() {
            0 => return None,
            1 => format!("{}, process.env.JWT_SECRET", parts[0]),
            _ => {
                // A second argument that is an options object is not a secret:
                // the secret belongs between the token and the options.
                let second = parts[1].trim();
                if second.starts_with('{') {
                    let rest = parts[1..].join(", ");
                    format!("{}, process.env.JWT_SECRET, {rest}", parts[0].trim())
                } else {
                    // The second argument already is a secret or a key.
                    args.clone()
                }
            }
        };

        edits.push(LineEdit::new(
            line_of(content, call_start),
            line_of(content, close),
            call_start,
            close + 1,
            format!("jwt.verify({new_args})"),
        ));
        cursor = close + 1;
    }

    edits_or_none(edits)
}

// ---------------------------------------------------------------------------
// SEC-INJ-001: concatenated SQL -> parameterised SQL
// ---------------------------------------------------------------------------

const SQL_CALL_NAMES: &[&str] = &["query", "execute", "exec", "raw", "run"];

fn fix_sql_injection(content: &str) -> Option<Vec<LineEdit>> {
    let bytes = content.as_bytes();
    let mut edits = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'(' {
            index += 1;
            continue;
        }

        let ident_start = identifier_start(content, index);
        let ident = &content[ident_start..index];
        let call_name = ident.rsplit('.').next().unwrap_or(ident);

        if SQL_CALL_NAMES.contains(&call_name) {
            if let Some(close) = find_matching_paren(content, index) {
                if let Some(new_args) = parameterize_sql(&content[index + 1..close]) {
                    edits.push(LineEdit::new(
                        line_of(content, ident_start),
                        line_of(content, close),
                        ident_start,
                        close + 1,
                        format!("{ident}({new_args})"),
                    ));
                    index = close + 1;
                    continue;
                }
            }
        }
        index += 1;
    }

    edits_or_none(edits)
}

/// Rewrite a single SQL argument into `(sql, [params])`.
fn parameterize_sql(args: &str) -> Option<String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return None;
    }
    if split_top_level_args(trimmed).len() != 1 {
        return None;
    }

    let (quote, inner, consumed) = parse_quoted(trimmed)?;
    let rest = &trimmed[consumed..];

    if quote == '`' {
        // Template literal: every `${ident}` becomes a `?` placeholder.
        let (sql, vars) = parameterize_template_literal(&inner)?;
        if vars.is_empty() {
            return None;
        }
        return Some(format!(
            "{}, [{}]",
            quote_as_double_quoted(&sql),
            vars.join(", ")
        ));
    }

    // `"literal" + identifier`
    let rest_trimmed = rest.trim();
    let plus = rest_trimmed.strip_prefix('+')?;
    let variable = plus.trim();
    if !is_simple_identifier(variable) {
        return None;
    }
    if inner.contains(quote) {
        return None;
    }
    let sql = if inner.ends_with(' ') {
        format!("{inner}?")
    } else {
        format!("{inner} ?")
    };
    Some(format!("{quote}{sql}{quote}, [{variable}]"))
}

/// Replace every `${identifier}` of a template literal with a `?` placeholder.
fn parameterize_template_literal(inner: &str) -> Option<(String, Vec<String>)> {
    let mut sql = String::new();
    let mut vars = Vec::new();
    let mut rest = inner;

    while let Some(open) = rest.find("${") {
        sql.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let close = after.find('}')?;
        let expression = after[..close].trim();
        if !is_simple_identifier(expression) {
            return None;
        }
        sql.push('?');
        vars.push(expression.to_string());
        rest = &after[close + 1..];
    }
    sql.push_str(rest);
    Some((sql, vars))
}

// ---------------------------------------------------------------------------
// SEC-TOK-002: hardcoded credential -> environment variable
// ---------------------------------------------------------------------------

const SECRET_NAME_MARKERS: &[&str] = &[
    "KEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTH",
];

const SECRET_PLACEHOLDERS: &[&str] = &[
    "example",
    "changeme",
    "change_me",
    "your_",
    "your-",
    "placeholder",
    "todo",
    "xxxx",
    "dummy",
    "fake",
    "process.env",
];

fn fix_hardcoded_secret(content: &str) -> Option<Vec<LineEdit>> {
    let bytes = content.as_bytes();
    let mut edits = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        if !is_identifier_start_byte(bytes[index]) {
            index += 1;
            continue;
        }

        let name_start = index;
        let mut name_end = index;
        while name_end < bytes.len() && is_identifier_byte(bytes[name_end]) {
            name_end += 1;
        }
        let name = &content[name_start..name_end];

        let mut cursor = name_end;
        while cursor < bytes.len() && (bytes[cursor] as char).is_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            index = name_end;
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && (bytes[cursor] as char).is_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let (quote, inner, consumed) = match parse_quoted(&content[cursor..]) {
            Some(parsed) => parsed,
            None => {
                index = name_end;
                continue;
            }
        };
        if quote == '`' {
            index = name_end;
            continue;
        }

        if is_secret_name(name) && is_credential_literal(&inner) {
            let env_name = screaming_snake(name);
            let literal_start = cursor;
            let literal_end = cursor + consumed;
            edits.push(LineEdit::new(
                line_of(content, literal_start),
                line_of(content, literal_end.saturating_sub(1)),
                literal_start,
                literal_end,
                format!("process.env.{env_name}"),
            ));
            index = literal_end;
            continue;
        }

        index = name_end;
    }

    edits_or_none(edits)
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_NAME_MARKERS
        .iter()
        .any(|marker| upper.contains(marker))
}

fn is_credential_literal(value: &str) -> bool {
    if value.len() < 12 || value.contains(char::is_whitespace) {
        return false;
    }
    if !value.chars().any(|ch| ch.is_ascii_digit()) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    !SECRET_PLACEHOLDERS
        .iter()
        .any(|marker| lower.contains(marker))
}

// ---------------------------------------------------------------------------
// SEC-TLS-005: rejectUnauthorized / minVersion
// ---------------------------------------------------------------------------

fn fix_tls(content: &str) -> Option<Vec<LineEdit>> {
    let mut edits = Vec::new();

    let reject_len = "rejectUnauthorized".len();
    let mut cursor = 0usize;
    while let Some(found) = content[cursor..].find("rejectUnauthorized") {
        let key_start = cursor + found;
        if let Some(index) = value_offset_after_key(content, key_start, reject_len) {
            if content[index..].starts_with("false") {
                edits.push(LineEdit::new(
                    line_of(content, index),
                    line_of(content, index + 4),
                    index,
                    index + 5,
                    "true",
                ));
                cursor = index + 5;
                continue;
            }
        }
        cursor = key_start + reject_len;
    }

    let min_version_len = "minVersion".len();
    let mut cursor = 0usize;
    while let Some(found) = content[cursor..].find("minVersion") {
        let key_start = cursor + found;
        if let Some(index) = value_offset_after_key(content, key_start, min_version_len) {
            if index >= content.len() {
                break;
            }
            if let Some((quote, inner, consumed)) = parse_quoted(&content[index..]) {
                if inner == "TLSv1" || inner == "TLSv1.1" {
                    let literal_end = index + consumed;
                    edits.push(LineEdit::new(
                        line_of(content, index),
                        line_of(content, literal_end.saturating_sub(1)),
                        index,
                        literal_end,
                        format!("{quote}TLSv1.2{quote}"),
                    ));
                    cursor = literal_end;
                    continue;
                }
            }
        }
        cursor = key_start + min_version_len;
    }

    edits_or_none(edits)
}

/// Offset of the first non-whitespace byte after the `:` of the key at `key_start`.
///
/// The key occupies `key_len` bytes starting at `key_start`. Returns `None`
/// when the key is not followed by a `:`. When the `:` is present but no value
/// follows it, the returned offset is `content.len()`.
fn value_offset_after_key(content: &str, key_start: usize, key_len: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut index = key_start + key_len;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index >= bytes.len() || bytes[index] != b':' {
        return None;
    }
    index += 1;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    Some(index)
}

// ---------------------------------------------------------------------------
// Shared text helpers
// ---------------------------------------------------------------------------

/// `None` when a template produced no edit at all, the edits otherwise.
fn edits_or_none(edits: Vec<LineEdit>) -> Option<Vec<LineEdit>> {
    if edits.is_empty() {
        None
    } else {
        Some(edits)
    }
}

/// 1-based line number of a byte offset.
fn line_of(content: &str, offset: usize) -> u32 {
    let upto = &content[..offset.min(content.len())];
    1 + upto.matches('\n').count() as u32
}

/// Index of the first byte of the identifier that ends right before `end`.
fn identifier_start(content: &str, end: usize) -> usize {
    let bytes = content.as_bytes();
    let mut start = end;
    while start > 0 {
        let byte = bytes[start - 1];
        if is_identifier_byte(byte) || byte == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    start
}

fn is_identifier_start_byte(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn is_simple_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !is_identifier_start_byte(bytes[0]) {
        return false;
    }
    bytes.iter().all(|byte| is_identifier_byte(*byte))
}

/// Split an argument list on top-level commas.
fn split_top_level_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in args.chars() {
        if let Some(active) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Index of the `)` that closes the `(` at `open`.
fn find_matching_paren(content: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (offset, ch) in content[open..].char_indices() {
        let index = open + offset;
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a leading quoted literal.
///
/// Returns the quote character, the unescaped inner text and the number of
/// bytes the literal occupies (quotes included).
fn parse_quoted(text: &str) -> Option<(char, String, usize)> {
    let mut chars = text.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '\'' && quote != '"' && quote != '`' {
        return None;
    }
    let mut inner = String::new();
    let mut escaped = false;
    let mut consumed = quote.len_utf8();

    for (_, ch) in chars {
        consumed += ch.len_utf8();
        if escaped {
            inner.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some((quote, inner, consumed));
        }
        inner.push(ch);
    }
    None
}

/// Render a SQL string as a double quoted JavaScript literal.
fn quote_as_double_quoted(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// `apiKey` -> `API_KEY`, `API_KEY` -> `API_KEY`.
fn screaming_snake(name: &str) -> String {
    let mut out = String::new();
    let mut previous_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_uppercase() && previous_lower {
            out.push('_');
        }
        previous_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        out.push(ch.to_ascii_uppercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use coderadar_domain::apply_edits;

    fn apply(content: &str, edits: &[LineEdit]) -> String {
        apply_edits(content, edits).expect("edits apply")
    }

    #[test]
    fn jwt_decode_becomes_jwt_verify_with_secret() {
        let content = "const decoded = jwt.decode(token);\n";
        let edits = fix_jwt_decode(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "const decoded = jwt.verify(token, process.env.JWT_SECRET);\n"
        );
    }

    #[test]
    fn jwt_decode_keeps_existing_secret_argument() {
        let content = "const decoded = jwt.decode(token, secret);\n";
        let edits = fix_jwt_decode(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(patched, "const decoded = jwt.verify(token, secret);\n");
    }

    #[test]
    fn jwt_decode_with_an_options_argument_gains_the_secret_before_it() {
        let content = "const decoded = jwt.decode(token, { complete: true });\n";
        let edits = fix_jwt_decode(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "const decoded = jwt.verify(token, process.env.JWT_SECRET, { complete: true });\n"
        );
    }

    #[test]
    fn jwt_decode_with_three_arguments_keeps_its_secret() {
        let content = "const decoded = jwt.decode(token, secret, { complete: true });\n";
        let edits = fix_jwt_decode(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "const decoded = jwt.verify(token, secret, { complete: true });\n"
        );
    }

    #[test]
    fn jwt_template_is_conservative() {
        assert!(fix_jwt_decode("const decoded = decode(token);\n").is_none());
    }

    #[test]
    fn concatenated_sql_becomes_parameterised() {
        let content = "db.query(\"SELECT * FROM users WHERE id = \" + userId);\n";
        let edits = fix_sql_injection(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "db.query(\"SELECT * FROM users WHERE id = ?\", [userId]);\n"
        );
    }

    #[test]
    fn interpolated_sql_becomes_parameterised() {
        let content = "db.query(`SELECT * FROM users WHERE id = ${userId}`);\n";
        let edits = fix_sql_injection(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "db.query(\"SELECT * FROM users WHERE id = ?\", [userId]);\n"
        );
    }

    #[test]
    fn sql_template_ignores_parameterised_queries() {
        assert!(
            fix_sql_injection("db.query(\"SELECT * FROM users WHERE id = ?\", [id]);\n").is_none()
        );
    }

    #[test]
    fn hardcoded_secret_becomes_environment_variable() {
        let content = "const apiKey = \"sk-live-9a8b7c6d5e4f3a2b1c0d\";\n";
        let edits = fix_hardcoded_secret(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(patched, "const apiKey = process.env.API_KEY;\n");
    }

    #[test]
    fn secret_template_ignores_placeholders() {
        let content = "const apiKey = \"your-api-key-here\";\n";
        assert!(fix_hardcoded_secret(content).is_none());
    }

    #[test]
    fn tls_template_hardens_configuration() {
        let content = "new https.Agent({ rejectUnauthorized: false, minVersion: 'TLSv1' });\n";
        let edits = fix_tls(content).expect("template matches");
        let patched = apply(content, &edits);
        assert_eq!(
            patched,
            "new https.Agent({ rejectUnauthorized: true, minVersion: 'TLSv1.2' });\n"
        );
    }

    #[test]
    fn every_template_has_a_rule_id() {
        for template in ALL_TEMPLATES {
            assert!(template.rule_id.len() > 4);
            assert!(!template.explanation.is_empty());
        }
        assert!(templated_rule_ids().contains(&"SEC-JWT-003"));
        assert!(template_for("NOPE-000-000").is_none());
    }
}
