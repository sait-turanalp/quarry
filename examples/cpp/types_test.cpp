/**
 * @file types_test.cpp
 * @brief Test file for struct and enum extraction
 */

/**
 * @brief Color enumeration
 */
enum Color {
    RED,
    GREEN,
    BLUE
};

/**
 * @brief Point structure
 */
struct Point {
    int x;
    int y;
};

/**
 * @brief Status codes
 */
enum class Status {
    SUCCESS,
    FAILURE,
    PENDING
};

/**
 * @brief Rectangle structure
 */
struct Rectangle {
    Point topLeft;
    Point bottomRight;
};
