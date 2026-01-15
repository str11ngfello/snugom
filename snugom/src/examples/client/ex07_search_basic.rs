//! Example 07 – Basic Search
//!
//! Demonstrates basic search operations:
//! - `find_many()` - Find entities matching a query
//! - `find_first()` - Find first matching entity
//! - `find_first_or_error()` - Find first or return error
//! - `count_where()` - Count matching entities
//! - `exists_where()` - Check if any match exists

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, SearchQuery};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "books")]
struct Book {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    #[snugom(filterable(text))]
    author: String,
    #[snugom(filterable(tag))]
    genre: String,
    #[snugom(filterable, sortable)]
    year: i64,
    #[snugom(filterable)]
    in_stock: bool,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Book])]
struct BookClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("search_basic");
    let mut client = BookClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut books = client.books();

    // Create test data
    let test_books = vec![
        Book::validation_builder()
            .title("The Rust Programming Language".to_string())
            .author("Steve Klabnik".to_string())
            .genre("programming".to_string())
            .year(2019)
            .in_stock(true)
            .created_at(Utc::now()),
        Book::validation_builder()
            .title("Programming Rust".to_string())
            .author("Jim Blandy".to_string())
            .genre("programming".to_string())
            .year(2021)
            .in_stock(true)
            .created_at(Utc::now()),
        Book::validation_builder()
            .title("Rust in Action".to_string())
            .author("Tim McNamara".to_string())
            .genre("programming".to_string())
            .year(2021)
            .in_stock(false)
            .created_at(Utc::now()),
        Book::validation_builder()
            .title("The Great Gatsby".to_string())
            .author("F. Scott Fitzgerald".to_string())
            .genre("fiction".to_string())
            .year(1925)
            .in_stock(true)
            .created_at(Utc::now()),
    ];

    books.create_many(test_books).await?;

    // ============ find_many() ============
    // Find all books in the "programming" genre
    let query = SearchQuery {
        filter: vec!["genre:eq:programming".to_string()],
        ..Default::default()
    };
    let result = books.find_many(query).await?;
    assert_eq!(result.items.len(), 3, "should find 3 programming books");

    // ============ find_first() ============
    // Find the first fiction book
    let query = SearchQuery {
        filter: vec!["genre:eq:fiction".to_string()],
        ..Default::default()
    };
    let first_fiction = books.find_first(query).await?;
    assert!(first_fiction.is_some(), "should find a fiction book");
    assert_eq!(first_fiction.unwrap().genre, "fiction");

    // Returns None if no match
    let query = SearchQuery {
        filter: vec!["genre:eq:mystery".to_string()],
        ..Default::default()
    };
    let no_match = books.find_first(query).await?;
    assert!(no_match.is_none(), "should return None for non-existent genre");

    // ============ find_first_or_error() ============
    // Guaranteed fetch or error
    let query = SearchQuery {
        filter: vec!["genre:eq:fiction".to_string()],
        ..Default::default()
    };
    let fiction_book = books.find_first_or_error(query).await?;
    assert_eq!(fiction_book.title, "The Great Gatsby");

    // ============ count_where() ============
    // Count books in stock (boolean fields use eq operator with true/false)
    let query = SearchQuery {
        filter: vec!["in_stock:eq:true".to_string()],
        ..Default::default()
    };
    let in_stock_count = books.count_where(query).await?;
    assert_eq!(in_stock_count, 3, "should have 3 books in stock");

    // ============ exists_where() ============
    // Check if any books from 2021 exist
    let query = SearchQuery {
        filter: vec!["year:range:2021,2021".to_string()],
        ..Default::default()
    };
    let has_2021 = books.exists_where(query).await?;
    assert!(has_2021, "should have books from 2021");

    // Check for non-existent year
    let query = SearchQuery {
        filter: vec!["year:range:2024,2024".to_string()],
        ..Default::default()
    };
    let has_2024 = books.exists_where(query).await?;
    assert!(!has_2024, "should not have books from 2024");

    Ok(())
}
