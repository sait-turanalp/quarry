/**
 * User authentication status enum
 * Defines possible authentication states
 */
export enum AuthStatus {
  /** User is logged in */
  AUTHENTICATED,
  /** User is not logged in */
  UNAUTHENTICATED,
  /** Authentication is being checked */
  PENDING
}

/**
 * User profile interface
 * Contains all user-related data
 */
export interface UserProfile {
  /** Unique user identifier */
  id: string;
  /** User's display name */
  name: string;
  /** User's email address */
  email: string;
}

/**
 * Configuration type alias
 * Maps configuration keys to values
 */
export type ConfigMap = Record<string, unknown>;

/**
 * User service class
 * Handles all user-related operations
 */
export class UserService {
  /**
   * Current user profile
   * Stores the authenticated user's data
   */
  private currentUser?: UserProfile;

  /**
   * Authentication token
   * JWT token for API requests
   */
  public token: string = '';

  /**
   * Get current user
   * Returns the currently authenticated user
   */
  getCurrentUser(): UserProfile | undefined {
    return this.currentUser;
  }

  /**
   * Set current user
   * Updates the current user profile
   */
  setCurrentUser(user: UserProfile): void {
    this.currentUser = user;
  }
}

/**
 * Utility function
 * Formats dates to ISO string
 */
export function formatDate(date: Date): string {
  return date.toISOString();
}

/**
 * Arrow function constant
 * Validates email addresses
 */
export const validateEmail = (email: string): boolean => {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
};

/**
 * Object with methods
 * String manipulation utilities
 */
export const StringUtils = {
  /**
   * Capitalize first letter
   * Converts first character to uppercase
   */
  capitalize: (str: string): string => {
    return str.charAt(0).toUpperCase() + str.slice(1);
  },

  /**
   * Truncate string
   * Limits string to specified length
   */
  truncate: (str: string, length: number): string => {
    return str.length > length ? str.slice(0, length) + '...' : str;
  }
};