import React from 'react';
import { Button } from './components/ui/button';

export function MyPage() {
  return (
    <div>
      <Button>Click me</Button>
    </div>
  );
}

export function AnotherComponent() {
  return <Button>Another</Button>;
}

export function* testGenerator() {
  yield 1;
  yield 2;
}
