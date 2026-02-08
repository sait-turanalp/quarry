/**
 * This is a regular function with JSDoc
 * It should work correctly
 */
function regularFunction(): void {
  console.log("I have JSDoc");
}

/**
 * This is a const arrow function with JSDoc
 * This JSDoc will be IGNORED - BUG!
 */
const arrowFunction = () => {
  console.log("My JSDoc is ignored");
};

/**
 * This is an exported const arrow function
 * This JSDoc will also be IGNORED
 */
export const exportedArrow = () => {
  console.log("My JSDoc is also ignored");
};