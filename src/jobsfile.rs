use crate::cron::{CronExpr, ParseError};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub expr: CronExpr,
    pub command: String,
    pub line: usize,
}

#[derive(Debug)]
pub enum JobsFileError {
    Parse { line: usize, source: ParseError },
    MissingCommand { line: usize },
}

impl fmt::Display for JobsFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobsFileError::Parse { line, source } => {
                write!(f, "line {line}: {source}")
            }
            JobsFileError::MissingCommand { line } => {
                write!(f, "line {line}: missing command after cron expression")
            }
        }
    }
}

impl std::error::Error for JobsFileError {}

/// Parses a jobs file: one job per line, `<cron expr>  <shell command>`.
/// Blank lines and lines starting with `#` are ignored.
pub fn parse_jobs(content: &str) -> Result<Vec<Job>, JobsFileError> {
    let mut jobs = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.splitn(6, char::is_whitespace).collect();
        if fields.len() < 5 {
            return Err(JobsFileError::Parse {
                line: line_no,
                source: ParseError::WrongFieldCount(fields.len()),
            });
        }
        let expr_str = fields[..5].join(" ");
        let expr = CronExpr::parse(&expr_str).map_err(|source| JobsFileError::Parse {
            line: line_no,
            source,
        })?;
        let command = fields.get(5).map(|s| s.trim()).unwrap_or("");
        if command.is_empty() {
            return Err(JobsFileError::MissingCommand { line: line_no });
        }
        jobs.push(Job {
            expr,
            command: command.to_string(),
            line: line_no,
        });
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_file() {
        let content = "\
# comment
*/15 * * * *  echo hi

0 9 * * 1 echo monday
";
        let jobs = parse_jobs(content).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].command, "echo hi");
        assert_eq!(jobs[1].command, "echo monday");
        assert_eq!(jobs[1].line, 4);
    }

    #[test]
    fn rejects_missing_command() {
        let err = parse_jobs("* * * * *\n").unwrap_err();
        assert!(matches!(err, JobsFileError::MissingCommand { line: 1 }));
    }

    #[test]
    fn rejects_bad_cron_expr() {
        let err = parse_jobs("60 * * * * echo hi\n").unwrap_err();
        assert!(matches!(err, JobsFileError::Parse { line: 1, .. }));
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let jobs = parse_jobs("\n# nothing here\n\n").unwrap();
        assert!(jobs.is_empty());
    }
}
