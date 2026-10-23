use super::{CodeKind, CodeSpec, DEFAULT_DIGITS, DEFAULT_PERIOD_S, Digest, SpecError};

#[test]
fn the_default_is_thirty_second_six_digit_sha1() {
    let spec = CodeSpec::default();

    assert_eq!(
        spec.kind(),
        CodeKind::TimeBased {
            period_s: DEFAULT_PERIOD_S
        }
    );
    assert_eq!(spec.digest(), Digest::Sha1);
    assert_eq!(spec.digits(), DEFAULT_DIGITS);
}

#[test]
fn digits_from_six_to_eight_are_accepted() {
    for digits in 6..=8 {
        assert!(
            CodeSpec::new(CodeKind::CounterBased, Digest::Sha256, digits).is_ok(),
            "{digits}"
        );
    }
}

#[test]
fn other_digit_counts_are_rejected() {
    for digits in [0, 5, 9, 10] {
        assert_eq!(
            CodeSpec::new(CodeKind::CounterBased, Digest::Sha1, digits),
            Err(SpecError::DigitsOutOfRange(digits)),
            "{digits}"
        );
    }
}

#[test]
fn periods_from_one_to_three_hundred_seconds_are_accepted() {
    for period_s in [1, 30, 60, 300] {
        let kind = CodeKind::TimeBased { period_s };
        assert!(CodeSpec::new(kind, Digest::Sha1, 6).is_ok(), "{period_s}");
    }
}

#[test]
fn other_periods_are_rejected() {
    for period_s in [0, 301, u16::MAX] {
        let kind = CodeKind::TimeBased { period_s };
        assert_eq!(
            CodeSpec::new(kind, Digest::Sha1, 6),
            Err(SpecError::PeriodOutOfRange(period_s)),
            "{period_s}"
        );
    }
}

#[test]
fn only_md5_is_marked_as_not_computable() {
    assert!(!Digest::Md5.can_compute());
    for digest in [Digest::Sha1, Digest::Sha256, Digest::Sha512] {
        assert!(digest.can_compute(), "{digest:?}");
    }
}
