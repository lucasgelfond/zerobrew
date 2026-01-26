/// Version parsing and comparison for Homebrew-style versions.
///
/// Handles versions like "8.0.1", "8.0.1_1" (rebuild suffix), etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Version segments (e.g., [8, 0, 1] for "8.0.1")
    segments: Vec<u64>,
    /// Rebuild number (e.g., 1 for "8.0.1_1", 0 for "8.0.1")
    rebuild: u32,
    /// Original version string
    original: String,
}

impl Version {
    /// Parse a version string into a Version struct.
    ///
    /// Supports formats like:
    /// - "8.0.1" -> segments=[8, 0, 1], rebuild=0
    /// - "8.0.1_1" -> segments=[8, 0, 1], rebuild=1
    /// - "2024.01.15" -> segments=[2024, 1, 15], rebuild=0
    pub fn parse(s: &str) -> Self {
        let original = s.to_string();

        // Split by underscore to separate rebuild suffix
        let (base, rebuild) = if let Some(idx) = s.rfind('_') {
            let (base_part, rebuild_part) = s.split_at(idx);
            let rebuild_str = &rebuild_part[1..]; // Skip the underscore
            let rebuild = rebuild_str.parse::<u32>().unwrap_or(0);
            (base_part, rebuild)
        } else {
            (s, 0)
        };

        // Parse version segments
        let segments: Vec<u64> = base
            .split(['.', '-'])
            .filter_map(|part| {
                // Extract leading numeric portion from each segment
                let numeric: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
                if numeric.is_empty() {
                    None
                } else {
                    numeric.parse::<u64>().ok()
                }
            })
            .collect();

        Self {
            segments,
            rebuild,
            original,
        }
    }

    /// Check if this version is newer than another.
    ///
    /// Comparison rules:
    /// - Compare segments numerically from left to right
    /// - 8.0.2 > 8.0.1_1 > 8.0.1 (base version takes precedence)
    /// - 8.0.1_1 > 8.0.1_0 == 8.0.1 (rebuild suffix is tiebreaker)
    pub fn is_newer_than(&self, other: &Self) -> bool {
        // Compare segments
        let max_len = self.segments.len().max(other.segments.len());

        for i in 0..max_len {
            let self_seg = self.segments.get(i).copied().unwrap_or(0);
            let other_seg = other.segments.get(i).copied().unwrap_or(0);

            if self_seg > other_seg {
                return true;
            }
            if self_seg < other_seg {
                return false;
            }
        }

        // Segments are equal, compare rebuild suffix
        self.rebuild > other.rebuild
    }

    /// Get the original version string.
    pub fn as_str(&self) -> &str {
        &self.original
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.is_newer_than(other) {
            std::cmp::Ordering::Greater
        } else if other.is_newer_than(self) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_version() {
        let v = Version::parse("8.0.1");
        assert_eq!(v.segments, vec![8, 0, 1]);
        assert_eq!(v.rebuild, 0);
        assert_eq!(v.as_str(), "8.0.1");
    }

    #[test]
    fn parse_version_with_rebuild() {
        let v = Version::parse("8.0.1_1");
        assert_eq!(v.segments, vec![8, 0, 1]);
        assert_eq!(v.rebuild, 1);
        assert_eq!(v.as_str(), "8.0.1_1");
    }

    #[test]
    fn parse_version_with_high_rebuild() {
        let v = Version::parse("30.2_2");
        assert_eq!(v.segments, vec![30, 2]);
        assert_eq!(v.rebuild, 2);
    }

    #[test]
    fn parse_date_style_version() {
        let v = Version::parse("2024.01.15");
        assert_eq!(v.segments, vec![2024, 1, 15]);
        assert_eq!(v.rebuild, 0);
    }

    #[test]
    fn newer_by_major_version() {
        let v1 = Version::parse("9.0.0");
        let v2 = Version::parse("8.0.1_1");
        assert!(v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn newer_by_minor_version() {
        let v1 = Version::parse("8.1.0");
        let v2 = Version::parse("8.0.9");
        assert!(v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn newer_by_patch_version() {
        let v1 = Version::parse("8.0.2");
        let v2 = Version::parse("8.0.1_1");
        assert!(v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn newer_by_rebuild_suffix() {
        let v1 = Version::parse("8.0.1_1");
        let v2 = Version::parse("8.0.1");
        assert!(v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn rebuild_zero_equals_no_rebuild() {
        let v1 = Version::parse("8.0.1_0");
        let v2 = Version::parse("8.0.1");
        assert!(!v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn equal_versions() {
        let v1 = Version::parse("8.0.1_1");
        let v2 = Version::parse("8.0.1_1");
        assert!(!v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
        assert_eq!(v1, v2);
    }

    #[test]
    fn different_segment_lengths() {
        let v1 = Version::parse("8.0.1.1");
        let v2 = Version::parse("8.0.1");
        assert!(v1.is_newer_than(&v2));

        let v3 = Version::parse("8.0");
        let v4 = Version::parse("8.0.1");
        assert!(v4.is_newer_than(&v3));
    }

    #[test]
    fn ordering_trait() {
        let v1 = Version::parse("8.0.2");
        let v2 = Version::parse("8.0.1_1");
        let v3 = Version::parse("8.0.1");

        assert!(v1 > v2);
        assert!(v2 > v3);
        assert!(v1 > v3);
    }

    #[test]
    fn realistic_homebrew_versions() {
        // emacs upgrade scenario
        let old = Version::parse("30.2");
        let new = Version::parse("30.2_2");
        assert!(new.is_newer_than(&old));

        // ffmpeg upgrade scenario
        let old = Version::parse("8.0.1");
        let new = Version::parse("8.0.2");
        assert!(new.is_newer_than(&old));

        // ripgrep upgrade scenario
        let old = Version::parse("14.0.0");
        let new = Version::parse("14.1.0");
        assert!(new.is_newer_than(&old));
    }
}
