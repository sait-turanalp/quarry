//! Database connection and management
//! 
//! This module demonstrates:
//! - External system integration patterns
//! - Resource management
//! - Error handling and conversion

use std::fmt;
use std::collections::HashMap;

/// Database connection manager
pub struct DatabaseConnection {
    connection_url: String,
    connected: bool,
    mock_data: HashMap<String, String>,
}

impl DatabaseConnection {
    /// Create new database connection
    pub fn new(connection_url: String) -> Self {
        Self {
            connection_url,
            connected: false,
            mock_data: HashMap::new(),
        }
    }
    
    /// Establish connection to database
    pub fn connect(&self) -> Result<(), DatabaseError> {
        if self.connection_url.is_empty() {
            return Err(DatabaseError::InvalidConnectionString);
        }
        
        // Mock connection establishment
        println!("Connecting to database: {}", self.connection_url);
        Ok(())
    }
    
    /// Close database connection
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.mock_data.clear();
        println!("Database connection closed");
    }
    
    /// Execute a database query
    pub fn execute(&self, query: &str, params: Option<Vec<String>>) -> Result<QueryResult, DatabaseError> {
        if !self.connected {
            return Err(DatabaseError::NotConnected);
        }
        
        if query.is_empty() {
            return Err(DatabaseError::InvalidQuery("Query cannot be empty".to_string()));
        }
        
        // Mock query execution
        let affected_rows = if query.to_uppercase().starts_with("SELECT") { 0 } else { 1 };
        
        Ok(QueryResult {
            affected_rows,
            last_insert_id: if query.to_uppercase().starts_with("INSERT") { Some(1) } else { None },
            data: vec![],
        })
    }
    
    /// Execute a prepared statement
    pub fn execute_prepared(&self, statement_id: u32, params: Vec<String>) -> Result<QueryResult, DatabaseError> {
        if !self.connected {
            return Err(DatabaseError::NotConnected);
        }
        
        // Mock prepared statement execution
        println!("Executing prepared statement {} with {} params", statement_id, params.len());
        
        Ok(QueryResult {
            affected_rows: 1,
            last_insert_id: None,
            data: vec![],
        })
    }
    
    /// Begin a transaction
    pub fn begin_transaction(&mut self) -> Result<Transaction, DatabaseError> {
        if !self.connected {
            return Err(DatabaseError::NotConnected);
        }
        
        Ok(Transaction::new())
    }
    
    /// Get connection status
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    
    /// Get connection URL (package-private)
    pub(crate) fn connection_url(&self) -> &str {
        &self.connection_url
    }
    
    /// Internal cleanup method
    fn cleanup_resources(&mut self) {
        self.mock_data.clear();
    }
}

/// Database transaction manager
pub struct Transaction {
    committed: bool,
}

impl Transaction {
    fn new() -> Self {
        Self { committed: false }
    }
    
    /// Commit the transaction
    pub fn commit(mut self) -> Result<(), DatabaseError> {
        self.committed = true;
        println!("Transaction committed");
        Ok(())
    }
    
    /// Rollback the transaction
    pub fn rollback(self) -> Result<(), DatabaseError> {
        println!("Transaction rolled back");
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.committed {
            println!("Transaction auto-rollback on drop");
        }
    }
}

/// Query execution result
#[derive(Debug)]
pub struct QueryResult {
    pub affected_rows: usize,
    pub last_insert_id: Option<u64>,
    pub data: Vec<HashMap<String, String>>,
}

impl QueryResult {
    /// Check if query affected any rows
    pub fn has_changes(&self) -> bool {
        self.affected_rows > 0
    }
    
    /// Get the number of result rows
    pub fn row_count(&self) -> usize {
        self.data.len()
    }
}

/// Database operation errors
#[derive(Debug, Clone)]
pub enum DatabaseError {
    NotConnected,
    InvalidConnectionString,
    InvalidQuery(String),
    ConnectionFailed(String),
    QueryExecutionFailed(String),
    TransactionFailed(String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseError::NotConnected => write!(f, "Database not connected"),
            DatabaseError::InvalidConnectionString => write!(f, "Invalid connection string"),
            DatabaseError::InvalidQuery(msg) => write!(f, "Invalid query: {}", msg),
            DatabaseError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            DatabaseError::QueryExecutionFailed(msg) => write!(f, "Query execution failed: {}", msg),
            DatabaseError::TransactionFailed(msg) => write!(f, "Transaction failed: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

// Module-level utility functions
pub fn validate_connection_string(connection_string: &str) -> bool {
    !connection_string.is_empty() && connection_string.contains("://")
}

pub fn escape_sql_identifier(identifier: &str) -> String {
    format!("`{}`", identifier.replace("`", "``"))
}

pub fn escape_sql_string(value: &str) -> String {
    format!("'{}'", value.replace("'", "''"))
}

// Module-level constants
pub const MAX_CONNECTION_TIMEOUT: u64 = 30; // seconds
pub const DEFAULT_PORT: u16 = 5432;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_database_connection() {
        let db = DatabaseConnection::new("test://localhost:5432/testdb".to_string());
        assert!(!db.is_connected());
        
        let result = db.connect();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_query_execution() {
        let mut db = DatabaseConnection::new("test://localhost:5432/testdb".to_string());
        db.connected = true; // Simulate connected state
        
        let result = db.execute("SELECT * FROM users", None);
        assert!(result.is_ok());
        
        let query_result = result.unwrap();
        assert_eq!(query_result.affected_rows, 0); // SELECT doesn't affect rows
    }
    
    #[test]
    fn test_transaction() {
        let mut db = DatabaseConnection::new("test://localhost:5432/testdb".to_string());
        db.connected = true;
        
        let transaction = db.begin_transaction();
        assert!(transaction.is_ok());
        
        let tx = transaction.unwrap();
        assert!(tx.commit().is_ok());
    }
    
    #[test]
    fn test_connection_string_validation() {
        assert!(validate_connection_string("postgresql://localhost:5432/db"));
        assert!(!validate_connection_string("invalid"));
        assert!(!validate_connection_string(""));
    }
    
    #[test]
    fn test_sql_escaping() {
        assert_eq!(escape_sql_identifier("table_name"), "`table_name`");
        assert_eq!(escape_sql_string("user's data"), "'user''s data'");
    }
}