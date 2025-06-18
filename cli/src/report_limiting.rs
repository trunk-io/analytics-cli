use clap::ValueEnum;

#[derive(ValueEnum, Debug, Clone, PartialEq, Default)]
pub enum ValidationReport {
    None,
    #[default]
    Limited,
    Full,
}

impl ValidationReport {
    const LIMITED_FILE_ISSUES_TO_SHOW: usize = 8;
    const LIMITED_FILES_TO_SHOW: usize = 8;

    pub fn limit_issues<I, T>(&self, issues: I) -> impl Iterator<Item = T>
    where
        I: Iterator<Item = T>,
    {
        match &self {
            ValidationReport::None => issues.take(0).collect::<Vec<T>>().into_iter(),
            ValidationReport::Limited => issues
                .take(Self::LIMITED_FILE_ISSUES_TO_SHOW)
                .collect::<Vec<T>>()
                .into_iter(),
            ValidationReport::Full => issues.collect::<Vec<T>>().into_iter(),
        }
    }

    pub fn limit_files<I, T>(&self, files: I) -> impl Iterator<Item = T>
    where
        I: Iterator<Item = T>,
    {
        match &self {
            ValidationReport::None => files.take(0).collect::<Vec<T>>().into_iter(),
            ValidationReport::Limited => files
                .take(Self::LIMITED_FILES_TO_SHOW)
                .collect::<Vec<T>>()
                .into_iter(),
            ValidationReport::Full => files.collect::<Vec<T>>().into_iter(),
        }
    }

    pub fn num_exceeding_files_limit(&self, num_files: usize) -> Option<usize> {
        match &self {
            ValidationReport::None => Some(num_files),
            ValidationReport::Limited => {
                if num_files > Self::LIMITED_FILES_TO_SHOW {
                    Some(num_files - Self::LIMITED_FILES_TO_SHOW)
                } else {
                    None
                }
            }
            ValidationReport::Full => None,
        }
    }
}
