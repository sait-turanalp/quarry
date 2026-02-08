/**
 * Add two numbers together
 * @param a First number
 * @param b Second number
 * @returns The sum of a and b
 */
export function add(a: number, b: number): number {
  return a + b;
}

// Another regular comment
export class Calculator {
  // Just a normal comment
  multiply(x: number, y: number): number {
    return x * y;
  }
  
  divide(x: number, y: number): number {
    if (y === 0) throw new Error("Division by zero");
    return x / y;
  }
}

export interface MathOperations {
  add: (a: number, b: number) => number;
  subtract: (a: number, b: number) => number;
}