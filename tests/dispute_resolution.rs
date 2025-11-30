use assert_cmd::Command;
use std::fs;
use tempfile::NamedTempFile;

// ============================================================================
// DISPUTE TESTS
// ============================================================================

#[test]
fn test_dispute_and_resolve() {
    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg("tests/fixtures/disputes.csv")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    
    // Client 1: 10 + 5 = 15, dispute resolved
    assert!(output_str.contains("1,15"));
    
    // Client 2: chargebacked, account should be locked
    let lines: Vec<&str> = output_str.lines().collect();
    let client2_line = lines.iter().find(|l| l.starts_with("2,")).unwrap();
    assert!(client2_line.ends_with(",true"));
}

#[test]
fn test_dispute_nonexistent_transaction() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,999\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Dispute ignored, balance unchanged
    assert!(output_str.contains("1,10.0000,0.0000,10.0000,false"));
}

#[test]
fn test_dispute_withdrawal_rejected() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,50.0\n\
         dispute,1,2\n",  // Try to dispute withdrawal
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Withdrawal dispute rejected, balance unchanged
    assert!(output_str.contains("1,50.0000,0.0000,50.0000,false"));
}

#[test]
fn test_dispute_wrong_client() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,2,1\n",  // Client 2 tries to dispute client 1's tx
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Dispute rejected due to client mismatch
    assert!(output_str.contains("1,100.0000,0.0000,100.0000,false"));
}

#[test]
fn test_double_dispute_same_transaction() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,1\n\
         dispute,1,1\n",  // Second dispute should be ignored
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Only one dispute processed
    assert!(output_str.contains("1,0.0000,10.0000,10.0000,false"));
}

#[test]
fn test_dispute_with_negative_balance() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,60.0\n\
         dispute,1,1\n",  // Dispute full $100 after withdrawing $60
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // available = 40 - 100 = -60, held = 100, total = 40
    assert!(output_str.contains("1,-60.0000,100.0000,40.0000,false"));
}

#[test]
fn test_multiple_disputes_different_transactions() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         dispute,1,1\n\
         dispute,1,2\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Both disputes succeed: available = 0, held = 150, total = 150
    assert!(output_str.contains("1,0.0000,150.0000,150.0000,false"));
}

// ============================================================================
// RESOLVE TESTS
// ============================================================================

#[test]
fn test_resolve_non_disputed_transaction() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         resolve,1,1\n",  // Resolve without dispute
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Resolve ignored, balance unchanged
    assert!(output_str.contains("1,10.0000,0.0000,10.0000,false"));
}

#[test]
fn test_resolve_then_dispute_again() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,1\n\
         resolve,1,1\n\
         dispute,1,1\n",  // Dispute again after resolve
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Should work: disputed again
    assert!(output_str.contains("1,0.0000,10.0000,10.0000,false"));
}

#[test]
fn test_resolve_after_chargeback() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,1\n\
         chargeback,1,1\n\
         resolve,1,1\n",  // Resolve after chargeback (TX removed)
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Account locked, resolve ignored
    let lines: Vec<&str> = output_str.lines().collect();
    let client1_line = lines.iter().find(|l| l.starts_with("1,")).unwrap();
    assert!(client1_line.ends_with(",true"));  // Still locked
}

// ============================================================================
// CHARGEBACK TESTS
// ============================================================================

#[test]
fn test_chargeback_non_disputed_transaction() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         chargeback,1,1\n",  // Chargeback without dispute
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Chargeback ignored, account not locked
    assert!(output_str.contains("1,10.0000,0.0000,10.0000,false"));
}

#[test]
fn test_chargeback_locks_account() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,1\n\
         chargeback,1,1\n\
         deposit,1,2,5.0\n",  // Should be rejected
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Account locked after chargeback
    let lines: Vec<&str> = output_str.lines().collect();
    let client1_line = lines.iter().find(|l| l.starts_with("1,")).unwrap();
    assert!(client1_line.ends_with(",true"));
}

#[test]
fn test_chargeback_with_negative_balance() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         withdrawal,1,2,60.0\n\
         dispute,1,1\n\
         chargeback,1,1\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // After chargeback: available=-60, held=0, total=-60, locked=true
    assert!(output_str.contains("1,-60.0000,0.0000,-60.0000,true"));
}

#[test]
fn test_multiple_chargebacks_blocked() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         dispute,1,1\n\
         chargeback,1,1\n\
         dispute,1,2\n",  // Should be rejected (account locked)
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Only first chargeback processed, second dispute rejected
    let lines: Vec<&str> = output_str.lines().collect();
    let client1_line = lines.iter().find(|l| l.starts_with("1,")).unwrap();
    assert!(client1_line.ends_with(",true"));  // Locked
}

// ============================================================================
// COMPLEX FLOWS
// ============================================================================

#[test]
fn test_dispute_resolve_dispute_cycle() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,10.0\n\
         dispute,1,1\n\
         resolve,1,1\n\
         dispute,1,1\n",  // Dispute again after resolve
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Should work: disputed again after resolve
    assert!(output_str.contains("1,0.0000,10.0000,10.0000,false"));
}

#[test]
fn test_multiple_disputes_with_mixed_resolution() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         deposit,1,3,25.0\n\
         dispute,1,1\n\
         dispute,1,2\n\
         resolve,1,1\n\
         dispute,1,3\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // After all operations: available=100, held=75 (tx2+tx3), total=175
    assert!(output_str.contains("1,100.0000,75.0000,175.0000,false"));
}

#[test]
fn test_complex_dispute_flow() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         withdrawal,1,3,30.0\n\
         dispute,1,1\n\
         resolve,1,1\n\
         withdrawal,1,4,20.0\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // 100 + 50 - 30 - 20 = 100
    assert!(output_str.contains("1,100.0000,0.0000,100.0000,false"));
}

// ============================================================================
// MULTI-CLIENT DISPUTE TESTS
// ============================================================================

#[test]
fn test_multiple_clients_with_disputes() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,2,2,200.0\n\
         dispute,1,1\n\
         dispute,2,2\n\
         resolve,1,1\n",  // Only client 1 resolves
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Client 1: resolved, available=100
    assert!(output_str.contains("1,100.0000,0.0000,100.0000,false"));
    // Client 2: still disputed, available=0, held=200
    assert!(output_str.contains("2,0.0000,200.0000,200.0000,false"));
}

#[test]
fn test_multiple_clients_interleaved_disputes() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,2,2,200.0\n\
         deposit,1,3,50.0\n\
         dispute,1,1\n\
         withdrawal,2,4,100.0\n\
         resolve,1,1\n\
         dispute,2,2\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Client 1: 100 + 50 = 150 (dispute resolved)
    assert!(output_str.contains("1,150.0000,0.0000,150.0000,false"));
    // Client 2: 200 - 100 = 100, then dispute 200 → available=-100, held=200, total=100
    assert!(output_str.contains("2,-100.0000,200.0000,100.0000,false"));
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_dispute_tiny_amount() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,0.0001\n\
         dispute,1,1\n",
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Tiny amount should work correctly
    assert!(output_str.contains("1,0.0000,0.0001,0.0001,false"));
}

#[test]
fn test_resolve_without_prior_dispute() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,1,2,50.0\n\
         resolve,1,1\n",  // Resolve without dispute
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg(temp_file.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Resolve ignored, total = 150
    assert!(output_str.contains("1,150.0000,0.0000,150.0000,false"));
}
