//! What the CLI prints, in the format asked for. Lines and columns are 1-based, and a column
//! counts characters (Unicode scalar values), not the UTF-16 units the LSP counts in.

use clap::ValueEnum;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// `path:line:col: message [code]`
    Text,
    /// An array of findings.
    Json,
    /// SARIF 2.1.0, for code scanning.
    Sarif,
    /// GitHub Actions workflow commands, which annotate the lines in a pull request.
    Github,
}

/// A 1-based line and character column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

impl Place {
    /// The place of byte `offset` in `text`.
    pub fn of(text: &str, offset: usize) -> Self {
        let before = &text[..offset];
        let line_start = before.rfind('\n').map_or(0, |index| index + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: before[line_start..].chars().count() + 1,
        }
    }
}

/// One thing to report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// As given, or relative to the working directory, with `/` between its parts as printed.
    pub path: String,
    /// Where it starts and ends; none for what is about the whole file, one it could not read.
    pub start: Option<Place>,
    pub end: Option<Place>,
    pub severity: Severity,
    /// The lint's or directive category's code, where it has one.
    pub code: Option<&'static str>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// `findings` as `format` prints them.
pub fn render(format: Format, findings: &[Finding]) -> String {
    match format {
        Format::Text => lines(findings, text),
        Format::Github => lines(findings, github),
        Format::Json => pretty(&json!(findings)),
        Format::Sarif => pretty(&sarif(findings)),
    }
}

fn lines(findings: &[Finding], line: fn(&Finding) -> String) -> String {
    findings
        .iter()
        .map(|finding| line(finding) + "\n")
        .collect()
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default() + "\n"
}

fn text(finding: &Finding) -> String {
    let at = finding
        .start
        .map(|start| format!(":{}:{}", start.line, start.column))
        .unwrap_or_default();
    let code = finding
        .code
        .map(|code| format!(" [{code}]"))
        .unwrap_or_default();
    format!("{}{at}: {}{code}", finding.path, finding.message)
}

/// `::warning file=…,line=…,col=…,endLine=…,endColumn=…,title=…::message`
fn github(finding: &Finding) -> String {
    let place = |key: &str, value: usize| format!("{key}={value}");
    let properties: Vec<String> = std::iter::once(format!("file={}", property(&finding.path)))
        .chain(
            finding
                .start
                .into_iter()
                .flat_map(|start| [place("line", start.line), place("col", start.column)]),
        )
        .chain(
            finding
                .end
                .into_iter()
                .flat_map(|end| [place("endLine", end.line), place("endColumn", end.column)]),
        )
        .chain(finding.code.map(|code| format!("title={}", property(code))))
        .collect();
    format!(
        "::{} {}::{}",
        finding.severity.name(),
        properties.join(","),
        data(&finding.message)
    )
}

/// A workflow command's message, escaped as the runner unescapes it.
fn data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// A workflow command's property value, which `:` and `,` end too.
fn property(text: &str) -> String {
    data(text).replace(':', "%3A").replace(',', "%2C")
}

/// A minimal SARIF 2.1.0 log: one run, a result per finding. Columns are characters, which the
/// run says, SARIF's default being UTF-16 units.
fn sarif(findings: &[Finding]) -> serde_json::Value {
    let results: Vec<serde_json::Value> = findings
        .iter()
        .map(|finding| {
            let mut region = serde_json::Map::new();
            if let Some(start) = finding.start {
                region.insert("startLine".into(), json!(start.line));
                region.insert("startColumn".into(), json!(start.column));
            }
            if let Some(end) = finding.end {
                region.insert("endLine".into(), json!(end.line));
                region.insert("endColumn".into(), json!(end.column));
            }
            let mut location = json!({ "artifactLocation": { "uri": finding.path } });
            if !region.is_empty() {
                location["region"] = region.into();
            }
            let mut result = json!({
                "level": finding.severity.name(),
                "message": { "text": finding.message },
                "locations": [{ "physicalLocation": location }],
            });
            if let Some(code) = finding.code {
                result["ruleId"] = json!(code);
            }
            result
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "janet-check",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": env!("CARGO_PKG_REPOSITORY"),
            }},
            "columnKind": "unicodeCodePoints",
            "results": results,
        }],
    })
}
