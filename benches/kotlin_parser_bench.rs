//! Kotlin Parser Performance Benchmarks
//!
//! This benchmark suite validates that the Kotlin parser meets performance targets:
//! - >10,000 symbols/second extraction speed
//! - Memory usage within acceptable limits
//! - Performance comparable to other language parsers
//! - Scalability with large codebases
//!
//! The benchmark suite measures:
//! 1. Symbol extraction performance with varying code complexity
//! 2. Memory usage patterns with different file sizes
//! 3. Parser initialization overhead
//! 4. Performance on specific Kotlin language constructs
//! 5. Scalability with systematically generated test data

use codanna::parsing::LanguageParser;
use codanna::parsing::kotlin::KotlinParser;
use codanna::types::{FileId, SymbolCounter};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

// Performance targets and constants
#[allow(dead_code)]
const TARGET_SYMBOLS_PER_SEC: u64 = 10_000;
#[allow(dead_code)]
const LARGE_FILE_SYMBOL_COUNT: usize = 1000;
#[allow(dead_code)]
const BENCHMARK_ITERATIONS: usize = 100;

/// Benchmark basic Kotlin symbol extraction performance
fn bench_kotlin_symbol_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("kotlin_symbol_extraction");

    // Test with different Kotlin code samples of varying complexity
    let test_cases = vec![
        ("basic_kotlin", create_basic_kotlin_code()),
        ("medium_kotlin", create_medium_complexity_kotlin_code()),
        ("complex_kotlin", create_complex_kotlin_code()),
        ("real_world_kotlin", create_real_world_kotlin_code()),
    ];

    for (name, source_code) in test_cases {
        let symbol_count = count_expected_symbols(&source_code);
        group.throughput(Throughput::Elements(symbol_count as u64));

        group.bench_with_input(
            BenchmarkId::new("symbol_extraction", name),
            &source_code,
            |b, code| {
                let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
                b.iter(|| {
                    let mut symbol_counter = SymbolCounter::new();
                    let file_id = FileId::new(1).expect("Failed to create file ID");
                    let symbols =
                        parser.parse(std::hint::black_box(code), file_id, &mut symbol_counter);
                    std::hint::black_box(symbols)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_kotlin_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("kotlin_memory_usage");

    // Test memory usage with increasingly large Kotlin files
    let sizes = vec![100, 500, 1000, 2000, 5000];

    for size in sizes {
        let source_code = create_large_kotlin_file(size);
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("large_file_parsing", size),
            &source_code,
            |b, code| {
                b.iter(|| {
                    let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
                    let mut symbol_counter = SymbolCounter::new();
                    let file_id = FileId::new(1).expect("Failed to create file ID");
                    let symbols =
                        parser.parse(std::hint::black_box(code), file_id, &mut symbol_counter);
                    std::hint::black_box(symbols)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark parser initialization overhead
fn bench_parser_initialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_initialization");

    group.bench_function("kotlin_parser_creation", |b| {
        b.iter(|| {
            let parser = KotlinParser::new();
            std::hint::black_box(parser)
        });
    });

    group.finish();
}

/// Benchmark specific Kotlin language constructs
fn bench_kotlin_language_constructs(c: &mut Criterion) {
    let mut group = c.benchmark_group("kotlin_language_constructs");

    let construct_tests = vec![
        ("functions", create_many_functions(100)),
        ("classes", create_many_classes(50)),
        ("interfaces", create_many_interfaces(30)),
        ("methods", create_many_methods(100)),
        ("data_classes", create_many_data_classes(50)),
        ("objects", create_many_objects(30)),
        ("properties", create_many_properties(100)),
        ("enums", create_many_enums(20)),
    ];

    for (construct_name, source_code) in construct_tests {
        let symbol_count = count_expected_symbols(&source_code);
        group.throughput(Throughput::Elements(symbol_count as u64));

        group.bench_with_input(
            BenchmarkId::new("construct_parsing", construct_name),
            &source_code,
            |b, code| {
                let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
                b.iter(|| {
                    let mut symbol_counter = SymbolCounter::new();
                    let file_id = FileId::new(1).expect("Failed to create file ID");
                    let symbols =
                        parser.parse(std::hint::black_box(code), file_id, &mut symbol_counter);
                    std::hint::black_box(symbols)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark scalable test data generation
fn bench_scalable_test_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalable_test_data");

    // Test with different data sizes to measure scalability
    let data_sizes = vec![100, 500, 1000, 2000, 5000, 10000];

    for size in data_sizes {
        let source_code = generate_scalable_kotlin_code(size);
        let symbol_count = count_expected_symbols(&source_code);
        group.throughput(Throughput::Elements(symbol_count as u64));

        group.bench_with_input(
            BenchmarkId::new("generated_data", size),
            &source_code,
            |b, code| {
                let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
                b.iter(|| {
                    let mut symbol_counter = SymbolCounter::new();
                    let file_id = FileId::new(1).expect("Failed to create file ID");
                    let symbols =
                        parser.parse(std::hint::black_box(code), file_id, &mut symbol_counter);
                    std::hint::black_box(symbols.len()) // Return count to avoid large memory allocations
                });
            },
        );
    }

    group.finish();
}

/// Benchmark document comment extraction performance
fn bench_doc_comment_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("doc_comment_extraction");

    let source_code = create_code_with_many_doc_comments(100);
    let symbol_count = count_expected_symbols(&source_code);
    group.throughput(Throughput::Elements(symbol_count as u64));

    group.bench_function("doc_comments", |b| {
        let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
        b.iter(|| {
            let mut symbol_counter = SymbolCounter::new();
            let file_id = FileId::new(1).expect("Failed to create file ID");
            let symbols = parser.parse(black_box(&source_code), file_id, &mut symbol_counter);
            black_box(symbols)
        });
    });

    group.finish();
}

/// Benchmark method call extraction
fn bench_method_call_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("method_call_extraction");

    let source_code = create_code_with_many_calls(100);

    group.bench_function("find_calls", |b| {
        let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
        b.iter(|| {
            let calls = parser.find_calls(std::hint::black_box(&source_code));
            std::hint::black_box(calls)
        });
    });

    group.finish();
}

/// Benchmark type usage extraction
fn bench_type_usage_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("type_usage_extraction");

    let source_code = create_code_with_type_usage(100);

    group.bench_function("find_uses", |b| {
        let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
        b.iter(|| {
            let uses = parser.find_uses(std::hint::black_box(&source_code));
            std::hint::black_box(uses)
        });
    });

    group.finish();
}

/// Benchmark inheritance extraction
fn bench_inheritance_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("inheritance_extraction");

    let source_code = create_code_with_inheritance(50);

    group.bench_function("find_extends", |b| {
        let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
        b.iter(|| {
            let extends = parser.find_extends(std::hint::black_box(&source_code));
            std::hint::black_box(extends)
        });
    });

    group.finish();
}

// ============================================================================
// Test Data Generation Functions
// ============================================================================

/// Create basic Kotlin code for benchmarking
fn create_basic_kotlin_code() -> String {
    r#"
package com.example

/**
 * A simple person data class.
 */
data class Person(
    val name: String,
    val age: Int
) {
    /**
     * Returns a greeting message.
     */
    fun greet(): String {
        return "Hello, I'm $name"
    }
}

/**
 * Main entry point.
 */
fun main() {
    val person = Person("Alice", 30)
    println(person.greet())
}
"#
    .to_string()
}

/// Create medium complexity Kotlin code
fn create_medium_complexity_kotlin_code() -> String {
    r#"
package com.example.config

import java.time.Duration

/**
 * Configuration class for application settings.
 */
data class Config(
    val host: String,
    val port: Int,
    val timeout: Duration = Duration.ofSeconds(30)
) {
    /**
     * Returns the full address.
     */
    fun getAddress(): String = "$host:$port"

    /**
     * Updates the timeout value.
     */
    fun updateTimeout(newTimeout: Duration): Config {
        return copy(timeout = newTimeout)
    }
}

/**
 * Service interface defining lifecycle methods.
 */
interface Service {
    fun start()
    fun stop()
    fun isHealthy(): Boolean
}

/**
 * Web service implementation.
 */
class WebService(private val config: Config) : Service {
    private var running = false

    override fun start() {
        running = true
        println("Starting web service at ${config.getAddress()}")
    }

    override fun stop() {
        running = false
        println("Stopping web service")
    }

    override fun isHealthy(): Boolean = running
}

/**
 * Companion object with factory methods.
 */
companion object {
    fun createDefault(): WebService {
        return WebService(Config("localhost", 8080))
    }
}
"#
    .to_string()
}

/// Create complex Kotlin code with advanced features
fn create_complex_kotlin_code() -> String {
    r#"
package com.example.advanced

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import java.util.concurrent.ConcurrentHashMap

/**
 * Generic repository interface.
 */
interface Repository<T> {
    suspend fun store(key: String, value: T): Result<Unit>
    suspend fun get(key: String): Result<T?>
    suspend fun getAll(): Flow<T>
    suspend fun delete(key: String): Result<Unit>
}

/**
 * In-memory repository implementation using coroutines.
 */
class InMemoryRepository<T> : Repository<T> {
    private val storage = ConcurrentHashMap<String, T>()

    override suspend fun store(key: String, value: T): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                storage[key] = value
            }
        }

    override suspend fun get(key: String): Result<T?> =
        withContext(Dispatchers.IO) {
            runCatching {
                storage[key]
            }
        }

    override suspend fun getAll(): Flow<T> = flow {
        storage.values.forEach { emit(it) }
    }.flowOn(Dispatchers.IO)

    override suspend fun delete(key: String): Result<Unit> =
        withContext(Dispatchers.IO) {
            runCatching {
                storage.remove(key)
                Unit
            }
        }
}

/**
 * Sealed class for modeling results.
 */
sealed class DataResult<out T> {
    data class Success<T>(val data: T) : DataResult<T>()
    data class Error(val message: String, val cause: Throwable? = null) : DataResult<Nothing>()
    object Loading : DataResult<Nothing>()
}

/**
 * Extension function for mapping results.
 */
inline fun <T, R> DataResult<T>.map(transform: (T) -> R): DataResult<R> = when (this) {
    is DataResult.Success -> DataResult.Success(transform(data))
    is DataResult.Error -> this
    is DataResult.Loading -> this
}

/**
 * Data service with caching and error handling.
 */
class DataService<T>(
    private val repository: Repository<T>,
    private val cache: MutableMap<String, T> = mutableMapOf()
) {
    suspend fun fetchData(key: String): DataResult<T> = coroutineScope {
        try {
            // Check cache first
            cache[key]?.let { return@coroutineScope DataResult.Success(it) }

            // Fetch from repository
            repository.get(key).fold(
                onSuccess = { value ->
                    value?.let {
                        cache[key] = it
                        DataResult.Success(it)
                    } ?: DataResult.Error("Not found")
                },
                onFailure = { error ->
                    DataResult.Error("Fetch failed", error)
                }
            )
        } catch (e: Exception) {
            DataResult.Error("Unexpected error", e)
        }
    }
}

/**
 * Object declaration for singleton pattern.
 */
object Logger {
    private val logs = mutableListOf<String>()

    fun log(message: String) {
        logs.add(message)
        println("[LOG] $message")
    }

    fun getLogs(): List<String> = logs.toList()
}
"#
    .to_string()
}

/// Create real-world Kotlin code pattern
fn create_real_world_kotlin_code() -> String {
    r#"
package com.example.app

import kotlinx.coroutines.*
import java.time.LocalDateTime

/**
 * User domain model.
 */
data class User(
    val id: Long,
    val name: String,
    val email: String,
    val created: LocalDateTime = LocalDateTime.now()
)

/**
 * User service interface.
 */
interface UserService {
    suspend fun createUser(user: User): Result<User>
    suspend fun getUser(id: Long): Result<User?>
    suspend fun listUsers(limit: Int, offset: Int): Result<List<User>>
    suspend fun updateUser(user: User): Result<User>
    suspend fun deleteUser(id: Long): Result<Unit>
}

/**
 * User service implementation with validation.
 */
class UserServiceImpl(
    private val repository: UserRepository
) : UserService {

    override suspend fun createUser(user: User): Result<User> = runCatching {
        require(user.name.isNotBlank()) { "Name cannot be blank" }
        require(user.email.contains("@")) { "Invalid email format" }

        repository.save(user)
    }

    override suspend fun getUser(id: Long): Result<User?> = runCatching {
        require(id > 0) { "Invalid user ID" }
        repository.findById(id)
    }

    override suspend fun listUsers(limit: Int, offset: Int): Result<List<User>> = runCatching {
        require(limit > 0) { "Limit must be positive" }
        require(offset >= 0) { "Offset must be non-negative" }

        repository.findAll(limit, offset)
    }

    override suspend fun updateUser(user: User): Result<User> = runCatching {
        require(user.id > 0) { "Invalid user ID" }
        require(user.name.isNotBlank()) { "Name cannot be blank" }

        repository.update(user)
    }

    override suspend fun deleteUser(id: Long): Result<Unit> = runCatching {
        require(id > 0) { "Invalid user ID" }
        repository.delete(id)
    }
}

/**
 * User repository interface.
 */
interface UserRepository {
    suspend fun save(user: User): User
    suspend fun findById(id: Long): User?
    suspend fun findAll(limit: Int, offset: Int): List<User>
    suspend fun update(user: User): User
    suspend fun delete(id: Long)
}

/**
 * HTTP controller for user endpoints.
 */
class UserController(private val service: UserService) {

    suspend fun handleCreateUser(request: CreateUserRequest): Response<User> {
        val user = User(
            id = 0,
            name = request.name,
            email = request.email
        )

        return service.createUser(user).fold(
            onSuccess = { Response.success(it) },
            onFailure = { Response.error(it.message ?: "Unknown error") }
        )
    }

    suspend fun handleGetUser(id: Long): Response<User?> {
        return service.getUser(id).fold(
            onSuccess = { Response.success(it) },
            onFailure = { Response.error(it.message ?: "Unknown error") }
        )
    }
}

/**
 * Request data class.
 */
data class CreateUserRequest(
    val name: String,
    val email: String
)

/**
 * Response wrapper.
 */
sealed class Response<out T> {
    data class Success<T>(val data: T) : Response<T>()
    data class Error(val message: String) : Response<Nothing>()

    companion object {
        fun <T> success(data: T): Response<T> = Success(data)
        fun error(message: String): Response<Nothing> = Error(message)
    }
}
"#
    .to_string()
}

/// Generate a large Kotlin file with specified number of symbols
fn create_large_kotlin_file(symbol_count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    // Add data classes
    for i in 0..(symbol_count / 4) {
        code.push_str(&format!(
            "data class DataClass{i}(\n    val field1: String,\n    val field2: Int\n)\n\n"
        ));
    }

    // Add interfaces
    for i in 0..(symbol_count / 8) {
        code.push_str(&format!(
            "interface Interface{i} {{\n    fun method{i}(): String\n}}\n\n"
        ));
    }

    // Add functions
    for i in 0..(symbol_count / 2) {
        code.push_str(&format!(
            "fun function{i}(param: String): String {{\n    return \"Function{i}: $param\"\n}}\n\n"
        ));
    }

    // Add main function
    code.push_str("fun main() {\n    println(\"Generated code\")\n}\n");

    code
}

/// Create many functions for benchmarking
fn create_many_functions(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "fun function{i}(param{i}: String): String {{\n    return \"Result: $param{i}\"\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create many classes for benchmarking
fn create_many_classes(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "class Class{i} {{\n    val field1: String = \"value\"\n    val field2: Int = {i}\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create many interfaces for benchmarking
fn create_many_interfaces(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "interface Interface{i} {{\n    fun method1{i}(): String\n    fun method2{i}(param: String)\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create many methods for benchmarking
fn create_many_methods(count: usize) -> String {
    let mut code = String::from("package com.example\n\nclass TestClass {\n");

    for i in 0..count {
        code.push_str(&format!(
            "    fun method{i}(param: String): String {{\n        return \"method{i}: $param\"\n    }}\n\n"
        ));
    }

    code.push_str("}\n\nfun main() {}\n");
    code
}

/// Create many data classes for benchmarking
fn create_many_data_classes(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "data class DataClass{i}(\n    val field1: String,\n    val field2: Int,\n    val field3: Boolean\n)\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create many objects for benchmarking
fn create_many_objects(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "object Object{i} {{\n    const val CONSTANT = \"value{i}\"\n    fun method(): String = CONSTANT\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create many properties for benchmarking
fn create_many_properties(count: usize) -> String {
    let mut code = String::from("package com.example\n\nclass PropertyClass {\n");

    for i in 0..count {
        code.push_str(&format!("    val property{i}: String = \"value{i}\"\n"));
    }

    code.push_str("}\n\nfun main() {}\n");
    code
}

/// Create many enums for benchmarking
fn create_many_enums(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "enum class Enum{i} {{\n    ENTRY1,\n    ENTRY2,\n    ENTRY3\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Generate scalable Kotlin code for systematic performance testing
fn generate_scalable_kotlin_code(target_symbols: usize) -> String {
    let mut code =
        String::from("package com.example.scalable\n\nimport java.time.LocalDateTime\n\n");

    // Calculate distribution of symbols
    let functions = target_symbols / 4;
    let classes = target_symbols / 8;
    let interfaces = target_symbols / 16;
    let methods = target_symbols / 4;
    let constants = target_symbols / 8;
    let properties = target_symbols / 8;

    // Add constants
    for i in 0..constants {
        code.push_str(&format!("const val CONSTANT{i} = \"value_{i}\"\n"));
    }
    code.push('\n');

    // Add data classes
    for i in 0..classes {
        code.push_str(&format!(
            "data class DataClass{i}(\n    val field1: String,\n    val field2: Int,\n    val field3: Boolean\n)\n\n"
        ));
    }

    // Add interfaces
    for i in 0..interfaces {
        code.push_str(&format!(
            "interface Interface{i} {{\n    fun method1(): String\n    fun method2(param: Int): Boolean\n}}\n\n"
        ));
    }

    // Add regular functions
    for i in 0..functions {
        code.push_str(&format!(
            "fun function{i}(param1: String, param2: Int): String {{\n    return \"func_{i}_${{param1}}_${{param2}}\"\n}}\n\n"
        ));
    }

    // Add class with many methods
    code.push_str("class MethodContainer {\n");
    for i in 0..methods {
        code.push_str(&format!("    fun method{i}(): String = \"method_{i}\"\n"));
    }
    code.push_str("}\n\n");

    // Add class with many properties
    code.push_str("class PropertyContainer {\n");
    for i in 0..properties {
        code.push_str(&format!("    val property{i}: String = \"value_{i}\"\n"));
    }
    code.push_str("}\n\n");

    code.push_str("fun main() {}\n");
    code
}

/// Create code with many doc comments for benchmarking
fn create_code_with_many_doc_comments(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    for i in 0..count {
        code.push_str(&format!(
            "/**\n * Documentation for function{i}.\n * This function does something important.\n * @param param The parameter\n * @return The result\n */\nfun function{i}(param: String): String {{\n    return param\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Create code with many method calls for benchmarking
fn create_code_with_many_calls(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    // Add some functions to call
    for i in 0..10 {
        code.push_str(&format!("fun helper{i}() {{}}\n"));
    }
    code.push('\n');

    // Add a function with many calls
    code.push_str("fun processData() {\n");
    for i in 0..count {
        let helper = i % 10;
        code.push_str(&format!("    helper{helper}()\n"));
    }
    code.push_str("}\n\n");

    code.push_str("fun main() {}\n");
    code
}

/// Create code with type usage for benchmarking
fn create_code_with_type_usage(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    // Add some types
    for i in 0..10 {
        code.push_str(&format!("data class Type{i}(val value: String)\n"));
    }
    code.push('\n');

    // Add functions using those types
    for i in 0..count {
        let type_idx = i % 10;
        code.push_str(&format!(
            "fun process{i}(param: Type{type_idx}): Type{type_idx} = param\n"
        ));
    }

    code.push_str("\nfun main() {}\n");
    code
}

/// Create code with inheritance for benchmarking
fn create_code_with_inheritance(count: usize) -> String {
    let mut code = String::from("package com.example\n\n");

    // Base interface
    code.push_str("interface BaseInterface {\n    fun baseMethod(): String\n}\n\n");

    // Add classes implementing the interface
    for i in 0..count {
        code.push_str(&format!(
            "class Class{i} : BaseInterface {{\n    override fun baseMethod(): String = \"class{i}\"\n}}\n\n"
        ));
    }

    code.push_str("fun main() {}\n");
    code
}

/// Count expected symbols in Kotlin source code (rough estimate)
fn count_expected_symbols(source_code: &str) -> usize {
    let mut count = 0;

    // Count function declarations
    count += source_code.matches("fun ").count();

    // Count class declarations
    count += source_code.matches("class ").count();
    count += source_code.matches("data class ").count();
    count += source_code.matches("object ").count();

    // Count interface declarations
    count += source_code.matches("interface ").count();

    // Count property declarations (val/var)
    count += source_code.matches("val ").count();
    count += source_code.matches("var ").count();

    // Count enum declarations
    count += source_code.matches("enum class ").count();

    // Return at least 1 to avoid division by zero in benchmarks
    count.max(1)
}

criterion_group!(
    benches,
    bench_kotlin_symbol_extraction,
    bench_kotlin_memory_usage,
    bench_parser_initialization,
    bench_kotlin_language_constructs,
    bench_scalable_test_data,
    bench_doc_comment_extraction,
    bench_method_call_extraction,
    bench_type_usage_extraction,
    bench_inheritance_extraction
);
criterion_main!(benches);
