use super::{
    KDF_MEMORY_KIB, KDF_PARALLELISM, KDF_TIME_COST, MasterError, derive_data_key, new_kdf_params,
};
use crate::sealed::KdfParams;

/// Cheap parameters so tests stay fast; production uses the recommended ones.
fn quick(salt: u8) -> KdfParams {
    KdfParams {
        salt: [salt; 16],
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    }
}

#[test]
fn the_same_password_and_parameters_give_the_same_key() {
    let first = derive_data_key("correct horse", &quick(1)).unwrap();
    let second = derive_data_key("correct horse", &quick(1)).unwrap();

    assert_eq!(first.expose(), second.expose());
}

#[test]
fn a_different_password_or_salt_gives_a_different_key() {
    let base = derive_data_key("correct horse", &quick(1)).unwrap();

    assert_ne!(
        derive_data_key("correct horsf", &quick(1))
            .unwrap()
            .expose(),
        base.expose()
    );
    assert_ne!(
        derive_data_key("correct horse", &quick(2))
            .unwrap()
            .expose(),
        base.expose()
    );
}

#[test]
fn an_empty_password_is_refused() {
    assert_eq!(
        derive_data_key("", &quick(1)).err(),
        Some(MasterError::EmptyPassword)
    );
}

#[test]
fn impossible_parameters_are_refused() {
    let too_little_memory = KdfParams {
        m_cost_kib: 1,
        ..quick(1)
    };
    let no_iterations = KdfParams {
        t_cost: 0,
        ..quick(1)
    };

    assert_eq!(
        derive_data_key("pw", &too_little_memory).err(),
        Some(MasterError::BadParams)
    );
    assert_eq!(
        derive_data_key("pw", &no_iterations).err(),
        Some(MasterError::BadParams)
    );
}

#[test]
fn new_parameters_use_the_recommended_costs_and_a_random_salt() {
    let first = new_kdf_params().unwrap();
    let second = new_kdf_params().unwrap();

    assert_eq!(
        (first.m_cost_kib, first.t_cost, first.p_cost),
        (KDF_MEMORY_KIB, KDF_TIME_COST, KDF_PARALLELISM)
    );
    assert_ne!(first.salt, second.salt);
}

#[test]
fn non_ascii_passwords_work() {
    let key = derive_data_key("主口令 pässwörd", &quick(3)).unwrap();

    assert_eq!(key.expose().len(), 32);
}

// The parameters are read from the file header before the file can be authenticated, so a tampered file
// must not be able to make the app allocate or compute without bound.
#[test]
fn parameters_from_a_tampered_file_are_capped() {
    let huge_memory = KdfParams {
        m_cost_kib: u32::MAX,
        ..quick(1)
    };
    let huge_time = KdfParams {
        t_cost: u32::MAX,
        ..quick(1)
    };
    let huge_lanes = KdfParams {
        p_cost: 1_000,
        m_cost_kib: 8_000,
        ..quick(1)
    };

    for params in [huge_memory, huge_time, huge_lanes] {
        assert_eq!(
            derive_data_key("pw", &params).err(),
            Some(MasterError::BadParams),
            "{params:?}"
        );
    }
}
