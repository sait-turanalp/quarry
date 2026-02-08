// Test file to verify alias resolution and relationships
import { Button } from '@/components/ui/button';
import { Container } from '@/components/Container';

export function TestComponent() {
    // Direct function call
    Button();
    Container();

    // These would be JSX in a real component
    // return <Button>Test</Button>;
}

export function AnotherTest() {
    TestComponent();  // This should create a relationship
}