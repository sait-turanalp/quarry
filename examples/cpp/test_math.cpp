/**
 * @file test_math.cpp
 * @brief Test program for math utilities
 */

#include "math_utils.h"
#include <iostream>

/**
 * @brief Calculate sum and product
 */
int calculate(int x, int y) {
    int sum = add(x, y);
    int product = multiply(x, y);
    return sum + product;
}

/**
 * @brief Main entry point
 */
int main() {
    int result = calculate(5, 3);
    std::cout << "Result: " << result << std::endl;

    // Direct call to multiply
    int direct = multiply(10, 20);
    std::cout << "Direct: " << direct << std::endl;

    return 0;
}
