use regex::Regex;

/// A version format detecting and comparing versions.
///
/// The extractor is built on a regular expression that extracts the numbers
/// to be used for the version. The goal is to have all relevant numbers captured
/// in [unnamed capture groups]. The [Regex] syntax is used.
///
/// Note that only unnamed capture groups will be extracted. Named capture groups have no effect.
/// You are responsible for ensuring that all capture groups only capture strings
/// that can be parsed into an unsigned integer. Otherwise, [`extract_from()`] will return `None`.
///
/// This also means that it is not possible to affect the ordering of the extracted numbers.
/// They will always be compared from left to right in the order of the capture groups. As an example,
/// it is not possible to extract a `<minor>.<major>` scheme, where you want to sort first by `<major>` and
/// then by `<minor>`. It will have to be sorted first by `<minor>` then by `<major>`, since `<minor>` is
/// before `<major>`.
///
/// # Examples
///
/// Detect only proper SemVer, without any prefix or suffix:
///
/// ```rust
/// # extern crate updock; use updock::VersionExtractor;
/// # fn main() {
/// let extractor = VersionExtractor::parse(r"^(\d+)\.(\d+)\.(\d+)$").unwrap();
/// assert!(extractor.matches("1.2.3"));
/// assert!(!extractor.matches("1.2.3-debian"));
/// # }
/// ```
///
/// Detect a sequential version after a prefix:
///
/// ```rust
/// # extern crate updock; use updock::VersionExtractor;
/// # fn main() {
/// let extractor = VersionExtractor::parse(r"^debian-r(\d+)$").unwrap();
/// assert!(extractor.matches("debian-r24"));
/// assert!(!extractor.matches("debian-r24-alpha"));
/// # }
/// ```
///
/// [unnamed capture groups]: https://docs.rs/regex/1.3.1/regex/#grouping-and-flags
/// [Regex]: https://docs.rs/regex/1.3.1/regex/index.html#syntax
/// [`extract_from()`]: #method.extract_from
#[derive(Debug)]
pub struct VersionExtractor {
    regex: Regex,
}

impl VersionExtractor {
    pub fn parse<S>(regex: S) -> Result<VersionExtractor, regex::Error>
    where
        S: AsRef<str>,
    {
        Ok(VersionExtractor {
            regex: Regex::new(regex.as_ref())?,
        })
    }

    pub fn from(regex: Regex) -> VersionExtractor {
        VersionExtractor { regex }
    }

    pub fn matches<S>(&self, candidate: S) -> bool
    where
        S: AsRef<str>,
    {
        self.regex.is_match(candidate.as_ref())
    }

    pub fn extract_from<S>(&self, candidate: S) -> Result<Option<Version>, ExtractionError>
    where
        S: AsRef<str>,
    {
        self.regex
            .captures_iter(candidate.as_ref())
            .flat_map(|capture| {
                capture
                    .iter()
                    .skip(1) // The first group is the entire match.
                    .filter_map(|maybe_submatch| {
                        maybe_submatch.map(|submatch| {
                            submatch
                                .as_str()
                                .parse()
                                .map_err(|_| ExtractionError::InvalidGroup)
                        })
                    })
                    .collect::<Vec<Result<VersionPart, ExtractionError>>>()
            })
            .collect::<Result<Vec<VersionPart>, ExtractionError>>()
            .map(Version::new)
    }

    pub fn filter<'a, S>(
        &'a self,
        candidates: impl IntoIterator<Item = S> + 'a,
    ) -> impl Iterator<Item = S> + 'a
    where
        S: AsRef<str>,
    {
        candidates
            .into_iter()
            .filter(move |candidate| self.matches(candidate.as_ref()))
    }

    pub fn extract<'a, S>(
        &'a self,
        candidates: impl IntoIterator<Item = S> + 'a,
    ) -> impl Iterator<Item = Result<(Version, S), ExtractionError>> + 'a
    where
        S: AsRef<str>,
    {
        candidates.into_iter().filter_map(move |candidate| {
            self.extract_from(candidate.as_ref())
                .transpose()
                .map(|result| result.map(|version| (version, candidate)))
        })
    }

    pub fn max<S>(
        &self,
        candidates: impl IntoIterator<Item = S>,
    ) -> Result<Option<S>, ExtractionError>
    where
        S: AsRef<str> + Ord,
    {
        let result: Result<Vec<(Version, S)>, ExtractionError> = self.extract(candidates).collect();
        result.map(|versions| {
            versions
                .into_iter()
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, tag)| tag)
        })
    }
}

// TODO: Test these errors..as_ref()
#[derive(Debug, PartialEq)]
pub enum ExtractionError {
    InvalidGroup,
    EmptyVersion,
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Version {
    parts: Vec<VersionPart>,
}

type VersionPart = u64;

impl Version {
    pub fn new(parts: Vec<VersionPart>) -> Option<Version> {
        if parts.is_empty() {
            None
        } else {
            Some(Version { parts })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proptest::prelude::*;

    macro_rules! prop_assert_matches {
        ($extractor:expr, $string:expr) => {
            prop_assert!(
                $extractor.matches($string),
                "{:?} did not match '{:?}'.",
                $extractor,
                $string
            );
        };
    }

    macro_rules! prop_assert_no_match {
        ($extractor:expr, $string:expr) => {
            prop_assert!(
                !$extractor.matches($string),
                "{:?} should not match '{}'.",
                $extractor,
                $string
            );
        };
    }

    fn strict_semver_extractor() -> VersionExtractor {
        VersionExtractor::parse(r"^(\d+)\.(\d+)\.(\d+)$").unwrap()
    }

    proptest! {
        #[test]
        fn detects_simple_semver(valid in r"[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+") {
            prop_assert_matches!(strict_semver_extractor(), &valid);
        }

        #[test]
        fn rejects_simple_semver_with_prefix(invalid in r"\PC*[^[:digit:]][[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+\PC*") {
            prop_assert_no_match!(strict_semver_extractor(), &invalid);
        }

        #[test]
        fn rejects_simple_semver_with_suffix(invalid in r"\PC*[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+[^[:digit:]]\PC*") {
            prop_assert_no_match!(strict_semver_extractor(), &invalid);
        }

        #[test]
        fn extracts_semver(major: u64, minor: u64, patch: u64, suffix in r"[^\d]\PC*") {
            let extractor = VersionExtractor::parse(r"(\d+)\.(\d+)\.(\d+)").unwrap();
            let candidate = format!("{}.{}.{}{}", major, minor, patch, suffix);
            let version = Version { parts: vec![major, minor, patch]};
            prop_assert_eq!(extractor.extract_from(&candidate), Ok(Some(version)));
        }

        #[test]
        fn retains_all_matching_semver_tags(tags in vec!(r"[[:digit:]]+\.[[:digit:]]+\.[[:digit:]]+")) {
            let extractor = strict_semver_extractor();
            let filtered: Vec<String> = extractor.filter(tags.clone()).collect();
            prop_assert_eq!(filtered, tags);
        }

        #[test]
        fn returns_correct_maximum(versions:Vec<(u64, u64, u64)>) {
            let tags = versions.iter().map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch));
            let extractor = strict_semver_extractor();
            let expected_max = versions.iter().max().map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch));
            prop_assert_eq!(extractor.max(tags), Ok(expected_max));
        }
    }

    #[test]
    fn removes_all_non_matching_tags() {
        let tags = vec!["1.2.3-debian", "1.2.3", "1.2", "1.2.2-debian", "1.2.2"];
        let extractor = strict_semver_extractor();
        let filtered: Vec<&str> = extractor.filter(tags).collect();
        let expected = vec!["1.2.3", "1.2.2"];
        assert_eq!(filtered, expected);
    }
}
