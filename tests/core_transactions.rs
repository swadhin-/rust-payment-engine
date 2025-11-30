use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::NamedTempFile;

// ============================================================================
// BASIC DEPOSIT & WITHDRAWAL TESTS
// ============================================================================

#[test]
fn test_basic_deposits_and_withdrawals() {
    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg("tests/fixtures/basic.csv")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    
    // Check header
    assert!(output_str.contains("client,available,held,total,locked"));
    
    // Client 1: deposited 3.0, withdrew 1.5 = 1.5 available
    assert!(output_str.contains("1,1.5"));
    
    // Client 2: deposited 2.0, tried to withdraw 3.0 (failed) = 2.0 available
    assert!(output_str.contains("2,2"));
}

#[test]
fn test_withdrawal_insufficient_funds() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\ndeposit,1,1,5.0\nwithdrawal,1,2,10.0\n",
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
    // Withdrawal should fail, balance should remain 5.0
    assert!(output_str.contains("1,5"));
}

#[test]
fn test_multiple_clients() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         deposit,2,2,200.0\n\
         withdrawal,1,3,50.0\n\
         deposit,3,4,300.0\n",
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
    
    // Verify all three clients
    assert!(output_str.contains("1,50"));
    assert!(output_str.contains("2,200"));
    assert!(output_str.contains("3,300"));
}

// ============================================================================
// INPUT VALIDATION TESTS
// ============================================================================

#[test]
fn test_missing_input_file() {
    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    cmd.arg("nonexistent.csv")
        .assert()
        .failure();
}

#[test]
fn test_empty_file() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), "type,client,tx,amount\n").unwrap();

    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    cmd.arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("client,available,held,total,locked"));
}

#[test]
fn test_whitespace_handling() {
    let mut cmd = Command::cargo_bin("payments-engine").unwrap();
    let output = cmd
        .arg("tests/fixtures/edge_cases.csv")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    
    // Client 2: 3.4567 deposited (with whitespace in CSV)
    assert!(output_str.contains("2,3.4567"));
}

#[test]
fn test_decimal_precision() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,1.2345\n\
         deposit,1,2,2.3456\n",
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
    
    // Should maintain 4 decimal precision: 1.2345 + 2.3456 = 3.5801
    assert!(output_str.contains("1,3.5801"));
}

// ============================================================================
// LOCKED ACCOUNT TESTS
// ============================================================================

#[test]
fn test_locked_account_rejects_deposits() {
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
    
    // Account should be locked, deposit after chargeback should fail
    let lines: Vec<&str> = output_str.lines().collect();
    let client1_line = lines.iter().find(|l| l.starts_with("1,")).unwrap();
    assert!(client1_line.ends_with(",true"));  // locked
    
    // Balance should be -10 (chargeback removed held $10)
    // Not +5 from the rejected deposit
    assert!(client1_line.contains(",-10.0000,") || client1_line.contains(",0.0000,"));
}

#[test]
fn test_locked_account_rejects_withdrawals() {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        "type,client,tx,amount\n\
         deposit,1,1,100.0\n\
         dispute,1,1\n\
         chargeback,1,1\n\
         withdrawal,1,2,5.0\n",  // Should be rejected
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
    
    // Account locked, withdrawal rejected
    let lines: Vec<&str> = output_str.lines().collect();
    let client1_line = lines.iter().find(|l| l.starts_with("1,")).unwrap();
    assert!(client1_line.ends_with(",true"));  // locked
}
