/**
 * Calculate the area of a rectangle
 * This function takes width and height parameters and returns the calculated area
 * UPDATED: Now with better documentation for testing re-indexing
 * @param width - The width of the rectangle in pixels
 * @param height - The height of the rectangle in pixels
 * @returns The area of the rectangle in square pixels
 */
export function calculateArea(width: number, height: number): number {
  return width * height;
}

/**
 * User authentication service
 * Handles user login, logout, and session management
 */
export class AuthService {
  /**
   * Authenticate a user with credentials
   * @param username - The user's username
   * @param password - The user's password
   * @returns True if authentication successful, false otherwise
   */
  async login(username: string, password: string): Promise<boolean> {
    // Authentication logic here
    return true;
  }

  /**
   * Log out the current user
   * Clears the session and removes authentication tokens
   */
  logout(): void {
    // Logout logic here
  }
}

/**
 * Configuration settings interface
 * Defines the structure for application configuration
 */
export interface Config {
  /** Database connection string */
  databaseUrl: string;
  /** Server port number */
  port: number;
  /** Enable debug mode */
  debug: boolean;
}