// Test file for alias resolution
import { Button } from '@/components/ui/button';
import { Container } from '@/components/Container';

export function TestComponent() {
    // Direct function calls (not JSX)
    Button();
    Container();

    return null;
}

export function AnotherTest() {
    TestComponent();
}