# SPDX-License-Identifier: AGPL-3.0-only
"""Reviewed completion contract for the existing real-library recovery matrix."""
import json


# Deliberately independent of the producer's CASES: changing or removing a case
# must be reviewed here rather than silently reducing required coverage.
CASES = frozenset('''
business::corrupt_missing_and_newer_database_backups_preserve_original_state
business::database_backup_and_restore_path_failures_keep_existing_data
business::matching_identity_and_business_restore_recover_only_the_original_owner
business::missing_outbox_and_configuration_are_not_reconstructed_by_store_restore
business::repeated_restore_rotates_realms_and_never_overwrites_existing_targets
business::restored_unknown_operations_cannot_become_new_current_line_actions
business::sms_and_calls_restore_preserve_ids_unknowns_and_invalidate_old_cursors
business::wal_backup_captures_committed_data_and_omits_other_backup_domains
business::wrong_owner_proof_never_reactivates_another_principal
identity::identity_ciphertext_key_pin_and_inner_version_matrix_writes_no_plaintext
identity::identity_restore_repetition_links_and_private_modes_do_not_overwrite
identity::mixed_device_and_missing_identity_fail_at_the_actual_auth_boundary
outbox::closed_outbox_reopen_preserves_confirmed_unknown_and_attempt_ids
outbox::restored_realm_rejects_both_matching_and_mixed_old_outbox_without_adoption
outbox::same_device_mixed_batches_have_no_product_wide_consistency_proof
'''.split())


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate recovery result key')
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError('nonfinite recovery result value')


def validate(text):
    """Reject plans, partial inventories, duplicate cases and changed boundaries."""
    try:
        row = json.loads(text, object_pairs_hook=unique_object, parse_constant=invalid_constant)
        expected = {'passed', 'backend', 'case_count', 'cases', 'production_changes',
                    'device_operations', 'transport_sends', 'new_cargo_dependencies'}
        if type(row) is not dict or set(row) != expected:
            raise ValueError('recovery completion object required')
        if row['passed'] is not True or row['backend'] != 'locked-production-rust-rlibs':
            raise ValueError('real-library recovery completion required')
        if row['production_changes'] is not False or row['new_cargo_dependencies'] is not False:
            raise ValueError('recovery boundary changed')
        for key, value in (('case_count', len(CASES)), ('device_operations', 0), ('transport_sends', 0)):
            if type(row[key]) is not int or row[key] != value:
                raise ValueError('recovery count or boundary mismatch')
        cases = row['cases']
        if (type(cases) is not list or len(cases) != len(CASES)
                or any(type(case) is not str for case in cases) or set(cases) != CASES):
            raise ValueError('complete reviewed recovery inventory required')
    except (ValueError, TypeError, RecursionError) as error:
        raise ValueError('invalid recovery completion evidence') from error
    return {'passed': len(CASES), 'ignored': 0}
