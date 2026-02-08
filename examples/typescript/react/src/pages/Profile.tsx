import React from 'react';
import { CustomButton } from '../test_documented_jsx';

/**
 * User profile page component
 * Displays user information with action buttons
 */
export function Profile() {
  return (
    <div>
      <h1>User Profile</h1>
      <p>Username: John Doe</p>
      <CustomButton onClick={() => console.log('edit profile')} />
      <CustomButton onClick={() => console.log('logout')} />
    </div>
  );
}

/**
 * Account deletion confirmation page
 * Allows users to permanently delete their account
 */
export function DeleteAccount() {
  return (
    <div>
      <h1>Delete Account</h1>
      <p>This action cannot be undone</p>
      <CustomButton onClick={() => console.log('confirm delete')} />
    </div>
  );
}
