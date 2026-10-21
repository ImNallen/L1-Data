use super::{Kind, Token, significant, transaction_at, word_at};
use crate::txn::TransactionPlan;

fn refused(reason: &str) -> String {
    format!("SQL Server Manual mode does not support {reason}. Use Auto-commit to run native explicit or mixed batches, or separate supported transaction commands from other statements with GO. No selected batch was executed.")
}

pub(crate) fn manual_transaction(text: &str) -> Result<TransactionPlan, String> {
    let tokens = significant(text)?;
    let tokens = &tokens[tokens.iter().position(Token::code).unwrap_or(tokens.len())..];
    if module_definition(tokens) {
        return Ok(TransactionPlan::BeginIfIdle);
    }
    if tokens.split(|t| t.text == ";").filter(|part| !part.is_empty()).all(control_statement) {
        return Ok(TransactionPlan::Native);
    }
    if (0..tokens.len()).any(|i| transaction_at(tokens, i).is_some()) {
        return Err(refused("conditional, mixed, or unrecognized transaction-control batches"));
    }
    if tokens.iter().any(|t| t.word("EXEC") || t.word("EXECUTE")) {
        return Err(refused("procedure calls or dynamic EXEC because their transaction behavior is unknown"));
    }
    if tokens.iter().enumerate().any(|(i, token)| {
        token.word("SET") && tokens[i + 1..].iter().take_while(|t| t.kind == Kind::Word || t.text == ",").any(|option| {
            ["IMPLICIT_TRANSACTIONS", "ANSI_DEFAULTS", "XACT_ABORT", "REMOTE_PROC_TRANSACTIONS", "TRANSACTION",
             "NOEXEC", "PARSEONLY", "FMTONLY", "SHOWPLAN_ALL", "SHOWPLAN_TEXT", "SHOWPLAN_XML"].iter().any(|word| option.word(word))
        })
    }) {
        return Err(refused("SET options that change transaction handling or suppress execution"));
    }
    let ordinary = [
        "SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "MERGE", "CREATE", "ALTER", "DROP", "TRUNCATE",
        "GRANT", "DENY", "REVOKE", "DECLARE", "SET", "BEGIN", "IF", "WHILE", "BREAK", "CONTINUE", "RETURN",
        "GOTO", "WAITFOR", "PRINT", "RAISERROR", "THROW", "OPEN", "FETCH", "CLOSE", "DEALLOCATE",
    ];
    if !ordinary.iter().any(|word| word_at(tokens, 0, word)) {
        return Err(refused("this batch form, including procedure calls without EXEC, because its transaction behavior is unknown"));
    }
    Ok(TransactionPlan::BeginIfIdle)
}

fn module_definition(tokens: &[Token<'_>]) -> bool {
    let object = if word_at(tokens, 0, "CREATE") {
        if word_at(tokens, 1, "OR") && word_at(tokens, 2, "ALTER") { 3 } else { 1 }
    } else if word_at(tokens, 0, "ALTER") {
        1
    } else {
        return false;
    };
    ["PROC", "PROCEDURE", "FUNCTION", "TRIGGER", "VIEW"].iter().any(|word| word_at(tokens, object, word))
}

fn control_statement(tokens: &[Token<'_>]) -> bool {
    let tran = |i| word_at(tokens, i, "TRAN") || word_at(tokens, i, "TRANSACTION");
    let name = |rest: &[Token<'_>]| rest.len() == 1 && matches!(rest[0].kind, Kind::Word | Kind::Ident | Kind::Variable);
    if word_at(tokens, 0, "BEGIN") {
        let at = if word_at(tokens, 1, "DISTRIBUTED") { 2 } else { 1 };
        tran(at) && (tokens.len() == at + 1 || name(&tokens[at + 1..]))
    } else if word_at(tokens, 0, "COMMIT") || word_at(tokens, 0, "ROLLBACK") {
        tokens.len() == 1
            || word_at(tokens, 1, "WORK") && tokens.len() == 2
            || tran(1) && (tokens.len() == 2 || name(&tokens[2..]))
    } else {
        word_at(tokens, 0, "SAVE") && tran(1) && name(&tokens[2..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_control_batches_keep_native_semantics() {
        for sql in [
            "BEGIN TRAN", "BEGIN TRANSACTION [named]", "BEGIN DISTRIBUTED TRANSACTION", "COMMIT", "COMMIT WORK",
            "COMMIT TRANSACTION @name", "ROLLBACK", "ROLLBACK WORK", "ROLLBACK TRANSACTION [point]", "SAVE TRAN point",
            ";BEGIN TRAN; BEGIN TRANSACTION; COMMIT; /* retain outer transaction */;",
        ] {
            assert_eq!(manual_transaction(sql), Ok(TransactionPlan::Native), "{sql}");
        }
    }

    #[test]
    fn module_definitions_and_ordinary_batches_need_a_manual_transaction() {
        for sql in [
            "CREATE PROCEDURE p AS BEGIN BEGIN TRAN; EXEC other; COMMIT END",
            ";/* module */ CREATE OR ALTER PROC [p] AS SET IMPLICIT_TRANSACTIONS OFF; ROLLBACK",
            "ALTER PROCEDURE p AS BEGIN TRAN; COMMIT",
            "CREATE TRIGGER t ON dbo.t AFTER INSERT AS ROLLBACK TRAN",
            "CREATE FUNCTION f() RETURNS int AS BEGIN RETURN 1 END",
            "CREATE VIEW v AS SELECT [COMMIT] FROM t",
            "DECLARE @x int=1; UPDATE t SET n=@x; SELECT @x",
            "IF 1=0 UPDATE t SET n=2; SELECT n FROM t",
            "SELECT [COMMIT], \"ROLLBACK\", N'EXEC p', @COMMIT, #EXEC; /* SET NOEXEC ON */",
            "SET NOCOUNT ON; SELECT 1",
        ] {
            assert_eq!(manual_transaction(sql), Ok(TransactionPlan::BeginIfIdle), "{sql}");
        }
    }

    #[test]
    fn ambiguous_control_opaque_calls_and_transaction_options_refuse() {
        for sql in [
            "IF 1=0 ROLLBACK TRAN; UPDATE t SET n=2",
            "UPDATE t SET n=2; COMMIT",
            "BEGIN TRAN; INSERT INTO t VALUES(1); COMMIT",
            "DECLARE @x int; BEGIN TRAN",
            "BEGIN TRAN BEGIN TRAN",
            "COMMIT; UPDATE t SET n=2",
            "ROLLBACK; UPDATE t SET n=2",
            "EXEC p", "EXECUTE p", "EXEC(N'ROLLBACK; UPDATE t SET n=2')",
            "dbo.p", "p @n=2", "[dbo].[p] @n=2", "[SELECT] @n=2", "sp_executesql N'ROLLBACK'",
            "DECLARE @n int; EXEC @n=p", "UNKNOWN FORM",
            "SET IMPLICIT_TRANSACTIONS OFF; UPDATE t SET n=2",
            "SET ANSI_PADDING, IMPLICIT_TRANSACTIONS ON",
            "SET ANSI_DEFAULTS OFF", "SET XACT_ABORT ON", "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
            "SET REMOTE_PROC_TRANSACTIONS ON", "SET NOEXEC ON", "SET PARSEONLY ON", "SET FMTONLY ON", "SET SHOWPLAN_XML ON",
        ] {
            let error = manual_transaction(sql).expect_err(sql);
            assert!(error.contains("Manual") && error.contains("Auto-commit") && error.contains("GO"), "{error}");
        }
    }
}
