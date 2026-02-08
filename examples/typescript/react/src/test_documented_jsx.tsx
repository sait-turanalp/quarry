import React from 'react';

/**
 * Reusable button component with click handling
 * Provides consistent styling and behavior
 */
export function CustomButton({ onClick }: { onClick: () => void }) {
  return <button onClick={onClick}>Click me</button>;
}

/**
 * Main dashboard page component
 * Displays user interface with interactive buttons
 */
export function Dashboard() {
  return (
    <div>
      <h1>Dashboard</h1>
      <CustomButton onClick={() => console.log('clicked')} />
    </div>
  );
}

/**
 * Settings page component
 * Allows users to configure application preferences
 */
export function Settings() {
  return (
    <div>
      <h1>Settings</h1>
      <CustomButton onClick={() => console.log('save')} />
    </div>
  );
}
