/**
 * @file comprehensive.c
 * @brief Comprehensive C example demonstrating all language constructs for parser testing
 * @author Code Intelligence System
 * @version 1.0
 * @date 2024
 * 
 * This file contains a comprehensive collection of C language features including:
 * - All data types (primitives, structures, unions, enums, function pointers)
 * - All control structures (if/else, switch, loops, goto)
 * - Function definitions with various signatures and storage classes
 * - Preprocessor directives and macro definitions
 * - Memory management and dynamic allocation
 * - Bit manipulation and low-level operations
 * - Variadic functions and advanced parameter handling
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdarg.h>
#include <signal.h>

/**
 * @def MAX_SIZE
 * @brief Maximum buffer size for operations
 */
#define MAX_SIZE 1024

/**
 * @def SQUARE(x)
 * @brief Macro to calculate square of a number
 * @param x The number to square
 * @return The square of x
 */
#define SQUARE(x) ((x) * (x))

/**
 * @def DEBUG_PRINT(fmt, ...)
 * @brief Debug printing macro with format string support
 * @param fmt Format string for printf
 * @param ... Variable arguments for format string
 */
#define DEBUG_PRINT(fmt, ...) printf("[DEBUG] " fmt "\n", ##__VA_ARGS__)

// Conditional preprocessing constructs
#ifdef DEBUG_BUILD
    #define LOG_ENABLED 1
    #define LOG(msg) printf("[LOG] %s\n", msg)
#else
    #define LOG_ENABLED 0
    #define LOG(msg)
#endif

#if defined(FEATURE_ADVANCED) || defined(FEATURE_EXTENDED)
    #define ADVANCED_FEATURES_AVAILABLE
    #ifdef FEATURE_ADVANCED
        #define ADVANCED_MODE 1
    #elif defined(FEATURE_EXTENDED)
        #define ADVANCED_MODE 2
    #else
        #define ADVANCED_MODE 0
    #endif
#endif

// Preprocessor function-like macro calls
#define CONCAT(a, b) a##b
#define STRINGIFY(x) #x
#define IS_DEFINED(x) defined(x)

// Forward declarations with documentation
struct Node;
typedef struct Node Node;

/**
 * @typedef compare_fn
 * @brief Function pointer type for comparison operations
 * @param a First element to compare
 * @param b Second element to compare
 * @return Negative if a < b, 0 if a == b, positive if a > b
 */
typedef int (*compare_fn)(const void *, const void *);

/**
 * @typedef user_id_t
 * @brief Type definition for user identifiers
 * Used throughout the system to represent unique user IDs
 */
typedef uint32_t user_id_t;

/**
 * @enum status_t
 * @brief Status codes for operation results
 * 
 * Enumeration defining standard return codes used across
 * the application for consistent error handling.
 */
typedef enum {
    STATUS_OK = 0,      /**< Operation completed successfully */
    STATUS_ERROR = -1,  /**< General error occurred */
    STATUS_INVALID = -2 /**< Invalid parameters provided */
} status_t;

/**
 * @struct Point
 * @brief Represents a 2D point with coordinates and optional label
 * 
 * Structure used for geometric calculations and coordinate tracking.
 * Contains floating-point coordinates and an optional descriptive label.
 */
struct Point {
    float x;            /**< X coordinate */
    float y;            /**< Y coordinate */
    const char *label;  /**< Optional descriptive label */
};

/**
 * @struct Rectangle
 * @brief Represents a rectangle with dimensions and origin point
 * 
 * Anonymous structure (typedef) containing width, height, and origin.
 * Used for geometric shape operations and area calculations.
 */
typedef struct {
    int width;          /**< Rectangle width in units */
    int height;         /**< Rectangle height in units */
    struct Point origin; /**< Origin point of the rectangle */
} Rectangle;

/**
 * @struct Node
 * @brief Doubly-linked list node structure
 * 
 * Self-referential structure for implementing doubly-linked lists.
 * Contains data payload and pointers to previous and next nodes.
 */
struct Node {
    int data;           /**< Integer data payload */
    struct Node *next;  /**< Pointer to next node */
    struct Node *prev;  /**< Pointer to previous node */
};

/**
 * @union Value
 * @brief Multi-type value container union
 * 
 * Union allowing storage of different data types in the same memory location.
 * Useful for variant types and memory-efficient data structures.
 */
union Value {
    int i;          /**< Integer value */
    float f;        /**< Floating-point value */
    char str[32];   /**< String value with fixed size */
    void *ptr;      /**< Generic pointer value */
};

/**
 * @struct PackedData
 * @brief Packed structure with attributes
 * 
 * Demonstrates attribute declarations for compiler-specific optimizations.
 * Uses __attribute__ to control memory layout and alignment.
 */
struct __attribute__((packed)) PackedData {
    char flag;      /**< Single byte flag */
    int value;      /**< 4-byte integer (no padding) */
    char data[3];   /**< 3-byte array */
} __attribute__((aligned(8)));

/**
 * @struct Config
 * @brief Configuration structure with default values
 */
struct Config {
    int max_connections;    /**< Maximum allowed connections */
    float timeout_seconds;  /**< Timeout in seconds */
    const char *host;       /**< Host address */
    int ports[8];          /**< Array of port numbers */
};

// Linkage specification for C++ compatibility
#ifdef __cplusplus
extern "C" {
#endif

// Function prototypes with documentation

/**
 * @brief Adds two integers and returns the result
 * @param a First integer operand
 * @param b Second integer operand
 * @return Sum of a and b
 * 
 * Simple arithmetic function demonstrating basic parameter passing
 * and return value handling in C.
 */
int add(int a, int b);

/**
 * @brief Prints point coordinates and label to stdout
 * @param p Pointer to Point structure to print
 * 
 * Safely prints point information with null pointer checking.
 * Handles optional label field gracefully.
 */
void print_point(const struct Point *p);

/**
 * @brief Creates and initializes a new Rectangle structure
 * @param w Width of the rectangle
 * @param h Height of the rectangle
 * @param origin Origin point for the rectangle
 * @return Pointer to newly allocated Rectangle, or NULL on failure
 * 
 * Dynamically allocates memory for a Rectangle and initializes
 * all fields. Caller is responsible for freeing returned memory.
 */
Rectangle *create_rectangle(int w, int h, struct Point origin);

/**
 * @brief Comparison function for integer sorting
 * @param a Pointer to first integer
 * @param b Pointer to second integer
 * @return Comparison result for qsort compatibility
 * 
 * Standard comparison function implementing the qsort interface.
 * Returns negative, zero, or positive value based on comparison result.
 */
int compare_ints(const void *a, const void *b);

/**
 * @brief Validates that a value is within acceptable range
 * @param value Integer value to validate
 * @return 1 if valid, 0 if invalid
 * 
 * Inline function for efficient bounds checking.
 * Checks if value is non-negative and below MAX_SIZE threshold.
 */
static inline int is_valid(int value);

// Global variables with documentation

/**
 * @var global_counter
 * @brief Static global counter for tracking operations
 * 
 * File-scoped static variable used internally for counting
 * various operations throughout the program execution.
 */
static int global_counter = 0;

/**
 * @var program_name
 * @brief External reference to program name string
 * 
 * External linkage variable typically set by the runtime
 * to contain the program's executable name.
 */
extern const char *program_name;

/**
 * @var signal_flag
 * @brief Atomic flag for signal handling
 * 
 * Volatile atomic variable used for safe communication
 * between signal handlers and main program flow.
 */
volatile sig_atomic_t signal_flag = 0;

// Function definitions with comprehensive documentation

/**
 * @brief Adds two integers and returns the result
 * 
 * This is the implementation of the add function declared above.
 * Demonstrates simple arithmetic operations and parameter handling.
 */
int add(int a, int b) {
    return a + b;
}

/**
 * @brief Safely prints point information with null checking
 * 
 * Implementation handles null pointer gracefully and provides
 * formatted output for point coordinates and optional label.
 */
void print_point(const struct Point *p) {
    if (p == NULL) {
        printf("Point is NULL\n");
        return;
    }
    printf("Point: (%.2f, %.2f) - %s\n", p->x, p->y, p->label ? p->label : "unnamed");
}

/**
 * @brief Dynamically creates and initializes a Rectangle
 * 
 * Allocates heap memory for a Rectangle structure and initializes
 * all fields with provided parameters. Includes error handling
 * for memory allocation failures.
 */
Rectangle *create_rectangle(int w, int h, struct Point origin) {
    Rectangle *rect = malloc(sizeof(Rectangle));
    if (!rect) {
        return NULL;
    }
    
    rect->width = w;
    rect->height = h;
    rect->origin = origin;
    
    return rect;
}

/**
 * @brief Standard integer comparison function for qsort
 * 
 * Implements the comparison interface required by qsort and bsearch.
 * Uses the standard three-way comparison idiom for integers.
 */
int compare_ints(const void *a, const void *b) {
    int ia = *(const int*)a;
    int ib = *(const int*)b;
    return (ia > ib) - (ia < ib);
}

/**
 * @brief Fast inline validation for integer values
 * 
 * Efficient bounds checking using inline expansion.
 * Validates that values are within the acceptable range.
 */
static inline int is_valid(int value) {
    return value >= 0 && value < MAX_SIZE;
}

/**
 * @brief Demonstrates complex control flow and data structures
 * 
 * This function showcases multiple C language features including:
 * - Array initialization and iteration
 * - Structure array initialization
 * - Various loop constructs (for, while, do-while)
 * - Switch statements with multiple cases
 * - Conditional compilation and macro usage
 * - Function pointer usage
 * - Union operations
 * - Goto statements for cleanup patterns
 * 
 * Used primarily for parser testing to ensure all language
 * constructs are properly recognized and processed.
 */
void complex_function(void) {
    // Local variable initialization with documentation
    int numbers[10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};  /**< Test array for sorting */
    
    /**
     * @var points
     * @brief Array of Point structures for geometric testing
     * 
     * Initialized with common coordinate values used in
     * geometric calculations and transformations.
     */
    struct Point points[] = {
        {0.0f, 0.0f, "origin"},
        {1.0f, 1.0f, "unit"},
        {-1.0f, -1.0f, "negative"}
    };
    
    // Control structure demonstrations with continue statements
    for (int i = 0; i < 10; i++) {
        if (numbers[i] % 3 == 0) {
            continue;  // Skip multiples of 3
        }
        if (numbers[i] % 2 == 0) {
            DEBUG_PRINT("Even number: %d", numbers[i]);
        } else {
            DEBUG_PRINT("Odd number: %d", numbers[i]);
        }
    }
    
    // Compound literal examples
    struct Point temp_point = (struct Point){.x = 42.0, .y = 24.0, .label = "compound"};
    print_point(&temp_point);
    
    // More compound literals with nested initialization
    Rectangle temp_rect = (Rectangle){
        .width = 100,
        .height = 50,
        .origin = (struct Point){10.0, 20.0, "rect_origin"}
    };
    
    // Designated initializers for arrays
    int sparse_array[10] = {
        [0] = 1,
        [3] = 4,
        [7] = 8,
        [9] = 10
    };
    
    // Complex designated initializer for struct
    struct Config default_config = {
        .max_connections = 100,
        .timeout_seconds = 30.5f,
        .host = "localhost",
        .ports = {80, 443, 8080, [7] = 9000}
    };
    
    // Switch statement with comprehensive cases
    status_t status = STATUS_OK;
    switch (status) {
        case STATUS_OK:
            printf("Operation successful\n");
            break;
        case STATUS_ERROR:
            printf("Error occurred\n");
            break;
        default:
            printf("Unknown status\n");
            break;
    }
    
    // While loop demonstration
    int count = 0;
    while (count < 3) {
        print_point(&points[count]);
        count++;
    }
    
    // Do-while loop for guaranteed execution
    int j = 0;
    do {
        printf("Value: %d\n", SQUARE(j));
        j++;
    } while (j < 3);
    
    // Goto statement for cleanup patterns
    if (global_counter > 100) {
        goto cleanup;
    }
    
    // Function pointer demonstration
    compare_fn cmp = compare_ints;
    qsort(numbers, 10, sizeof(int), cmp);
    
    // Union usage demonstration
    union Value val;
    val.i = 42;
    printf("Integer value: %d\n", val.i);
    val.f = 3.14f;
    printf("Float value: %.2f\n", val.f);
    
cleanup:
    printf("Cleanup completed\n");
}

/**
 * @brief Variadic function for flexible logging
 * @param format Printf-style format string
 * @param ... Variable number of arguments matching format
 * 
 * Demonstrates variadic function implementation using va_list.
 * Provides flexible logging interface with format string support.
 * Uses standard library macros for argument processing.
 */
void debug_log(const char *format, ...) {
    va_list args;
    va_start(args, format);
    vprintf(format, args);
    va_end(args);
    printf("\n");
}

/**
 * @brief Reverses the bit pattern of a 32-bit unsigned integer
 * @param num Input number to reverse
 * @return Number with reversed bit pattern
 * 
 * Implements bit manipulation algorithm to reverse the order
 * of bits in a 32-bit unsigned integer. Demonstrates bitwise
 * operations, loops, and bit shifting techniques.
 */
uint32_t reverse_bits(uint32_t num) {
    uint32_t result = 0;
    for (int i = 0; i < 32; i++) {
        if (num & (1U << i)) {
            result |= (1U << (31 - i));
        }
    }
    return result;
}

/**
 * @brief Main program entry point
 * @param argc Number of command-line arguments
 * @param argv Array of command-line argument strings
 * @return Program exit status (EXIT_SUCCESS or EXIT_FAILURE)
 * 
 * Main function demonstrates:
 * - Command-line argument processing
 * - Function calls with various parameter types
 * - Structure usage and initialization
 * - Dynamic memory management
 * - Error handling and resource cleanup
 * - Integration of all implemented features
 * 
 * This function serves as the primary test driver for all
 * language features implemented in this comprehensive example.
 */
int main(int argc, char *argv[]) {
    printf("C Comprehensive Example\n");
    printf("Arguments: %d\n", argc);
    
    if (argc > 1) {
        printf("First argument: %s\n", argv[1]);
    }
    
    // Test basic arithmetic operations
    int sum = add(5, 3);
    printf("Sum: %d\n", sum);
    
    // Test structure usage and pointer operations
    struct Point p = {10.5f, 20.3f, "test point"};
    print_point(&p);
    
    // Test dynamic memory allocation and management
    Rectangle *rect = create_rectangle(100, 200, p);
    if (rect) {
        printf("Rectangle: %dx%d at (%.1f, %.1f)\n", 
               rect->width, rect->height, 
               rect->origin.x, rect->origin.y);
        free(rect);  // Proper memory cleanup
    }
    
    // Test complex language features
    complex_function();
    
    // Test bit manipulation algorithms
    uint32_t original = 0x12345678;
    uint32_t reversed = reverse_bits(original);
    printf("Original: 0x%08X, Reversed: 0x%08X\n", original, reversed);
    
    // Test variadic function capabilities
    debug_log("Debug message with values: %d, %s, %.2f", 42, "hello", 3.14);
    
    // Preprocessor function-like macro calls
    int concat_result = CONCAT(12, 34);  // Results in 1234
    const char *stringified = STRINGIFY(MAX_SIZE);  // Results in "1024"
    
#ifdef DEBUG_BUILD
    LOG("Debug build detected");
#endif
    
    // Use preprocessor defined checks
    #if IS_DEFINED(ADVANCED_FEATURES_AVAILABLE)
        printf("Advanced features are available\n");
    #endif
    
    return EXIT_SUCCESS;
}

/**
 * @brief Static helper function with internal linkage
 * 
 * Demonstrates static function declaration with file scope.
 * Uses static local variable for state preservation across calls.
 * Shows function-level static storage duration.
 */
static void helper_function(void) {
    static int call_count = 0;  /**< Static counter preserves state */
    call_count++;
    printf("Helper called %d times\n", call_count);
}

// Close linkage specification
#ifdef __cplusplus
}
#endif