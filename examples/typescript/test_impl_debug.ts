// Simple test for implementation tracking

interface ITest {
    test(): void;
}

class TestClass implements ITest {
    test(): void {
        console.log("test");
    }
}