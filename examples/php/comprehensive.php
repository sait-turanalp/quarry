<?php
/**
 * Comprehensive PHP test file for parser maturity assessment
 * Tests all major PHP language features and constructs
 */

declare(strict_types=1);

namespace App\Examples\Comprehensive;

// Use statements
use App\Core\BaseController;
use App\Models\User;
use App\Services\{AuthService, CacheService, LoggerService};
use App\Contracts\ServiceInterface as Service;
use function array_map;
use function array_filter;
use const PHP_VERSION;
use const E_ALL;

// Global constants
const MAX_SIZE = 1024;
define('APP_NAME', 'Comprehensive Test');
define('APP_VERSION', '1.0.0');

// Global variables
$globalCounter = 0;
$GLOBALS['app_config'] = [];

/**
 * Simple function with type hints
 */
function simpleFunction(int $x, int $y): int {
    return $x + $y;
}

/**
 * Function with optional and nullable parameters
 */
function complexFunction(
    string $name,
    ?int $age = null,
    bool $active = true,
    mixed $data = null
): array {
    return [
        'name' => $name,
        'age' => $age,
        'active' => $active,
        'data' => $data
    ];
}

/**
 * Variadic function
 */
function variadicFunction(string $format, ...$args): string {
    return sprintf($format, ...$args);
}

/**
 * Function with union types (PHP 8.0+)
 */
function unionTypes(int|string $id, array|object $data): int|string|null {
    if (is_int($id)) {
        return $id * 2;
    }
    return strtoupper($id);
}

/**
 * Function with intersection types (PHP 8.1+)
 */
function intersectionTypes(Countable&Iterator $collection): int {
    return count($collection);
}

/**
 * Arrow function (PHP 7.4+)
 */
$multiply = fn($x, $y) => $x * $y;
$filterPositive = fn($n) => $n > 0;

/**
 * Named arguments (PHP 8.0+)
 */
function namedParams(
    string $name,
    int $age,
    string $email,
    bool $active = true
): User {
    return new User(
        name: $name,
        age: $age,
        email: $email,
        active: $active
    );
}

/**
 * Interface with constants and methods
 */
interface ServiceInterface {
    public const VERSION = '1.0';
    public const DEFAULT_TIMEOUT = 30;
    
    public function execute(array $params): mixed;
    public function validate(mixed $data): bool;
    public function getName(): string;
}

/**
 * Interface extending another interface
 */
interface CacheableInterface extends ServiceInterface {
    public function getCacheKey(): string;
    public function getCacheTtl(): int;
}

/**
 * Trait with properties and methods
 */
trait TimestampableTrait {
    protected DateTime $createdAt;
    protected ?DateTime $updatedAt = null;
    
    public function touch(): void {
        $this->updatedAt = new DateTime();
    }
    
    public function getCreatedAt(): DateTime {
        return $this->createdAt;
    }
    
    abstract public function save(): bool;
}

/**
 * Trait using another trait
 */
trait LoggableTrait {
    use TimestampableTrait;
    
    protected ?LoggerService $logger = null;
    
    public function log(string $message, string $level = 'info'): void {
        $this->logger?->log($level, $message);
    }
}

/**
 * Abstract class
 */
abstract class BaseModel {
    protected int $id;
    protected array $attributes = [];
    
    abstract public function save(): bool;
    abstract public function delete(): bool;
    
    public function getAttribute(string $key): mixed {
        return $this->attributes[$key] ?? null;
    }
    
    public function setAttribute(string $key, mixed $value): void {
        $this->attributes[$key] = $value;
    }
}

/**
 * Simple class with various visibility modifiers
 */
class SimpleClass {
    public string $publicProperty = 'public';
    protected string $protectedProperty = 'protected';
    private string $privateProperty = 'private';
    
    public static string $staticProperty = 'static';
    public const CLASS_CONSTANT = 'constant';
    
    // Constructor property promotion (PHP 8.0+)
    public function __construct(
        public readonly string $name,
        private int $id,
        protected ?array $data = null
    ) {
        $this->createdAt = new DateTime();
    }
    
    public function publicMethod(): void {
        echo "Public method\n";
    }
    
    protected function protectedMethod(): void {
        echo "Protected method\n";
    }
    
    private function privateMethod(): void {
        echo "Private method\n";
    }
    
    public static function staticMethod(): void {
        echo "Static method\n";
    }
    
    final public function finalMethod(): void {
        echo "Final method\n";
    }
}

/**
 * Class with magic methods
 */
class MagicClass {
    private array $data = [];
    
    public function __get(string $name): mixed {
        return $this->data[$name] ?? null;
    }
    
    public function __set(string $name, mixed $value): void {
        $this->data[$name] = $value;
    }
    
    public function __isset(string $name): bool {
        return isset($this->data[$name]);
    }
    
    public function __unset(string $name): void {
        unset($this->data[$name]);
    }
    
    public function __call(string $method, array $args): mixed {
        echo "Called method: $method\n";
        return null;
    }
    
    public static function __callStatic(string $method, array $args): mixed {
        echo "Called static method: $method\n";
        return null;
    }
    
    public function __toString(): string {
        return json_encode($this->data);
    }
    
    public function __invoke(...$args): mixed {
        return $this->process(...$args);
    }
    
    public function __debugInfo(): array {
        return ['data' => $this->data];
    }
    
    public function __sleep(): array {
        return ['data'];
    }
    
    public function __wakeup(): void {
        // Reinitialize after unserialization
    }
    
    public function __serialize(): array {
        return $this->data;
    }
    
    public function __unserialize(array $data): void {
        $this->data = $data;
    }
    
    public function __clone(): void {
        $this->data = array_map(fn($item) => 
            is_object($item) ? clone $item : $item, 
            $this->data
        );
    }
    
    private function process(...$args): mixed {
        return $args;
    }
}

/**
 * Class extending abstract class and implementing interface
 */
class UserModel extends BaseModel implements ServiceInterface, \JsonSerializable {
    use LoggableTrait;
    
    private string $name;
    private string $email;
    
    public function __construct(string $name, string $email) {
        $this->name = $name;
        $this->email = $email;
        $this->createdAt = new DateTime();
    }
    
    public function save(): bool {
        $this->log("Saving user: {$this->name}");
        return true;
    }
    
    public function delete(): bool {
        $this->log("Deleting user: {$this->name}");
        return true;
    }
    
    public function execute(array $params): mixed {
        return $this->save();
    }
    
    public function validate(mixed $data): bool {
        return !empty($data['name']) && !empty($data['email']);
    }
    
    public function getName(): string {
        return $this->name;
    }
    
    public function jsonSerialize(): array {
        return [
            'name' => $this->name,
            'email' => $this->email,
            'created_at' => $this->createdAt->format('Y-m-d H:i:s')
        ];
    }
}

/**
 * Final class (cannot be extended)
 */
final class FinalClass {
    public function process(): void {
        echo "Processing in final class\n";
    }
}

/**
 * Generic-like class using templates in docblocks
 * @template T
 */
class Container {
    /** @var array<T> */
    private array $items = [];
    
    /**
     * @param T $item
     */
    public function add($item): void {
        $this->items[] = $item;
    }
    
    /**
     * @return T|null
     */
    public function get(int $index) {
        return $this->items[$index] ?? null;
    }
    
    /**
     * @return array<T>
     */
    public function all(): array {
        return $this->items;
    }
}

/**
 * Enum (PHP 8.1+)
 */
enum Status: string {
    case PENDING = 'pending';
    case PROCESSING = 'processing';
    case COMPLETED = 'completed';
    case FAILED = 'failed';
    
    public function isFinished(): bool {
        return $this === self::COMPLETED || $this === self::FAILED;
    }
}

/**
 * Backed enum with methods
 */
enum Priority: int {
    case LOW = 1;
    case MEDIUM = 5;
    case HIGH = 10;
    case CRITICAL = 15;
    
    public function getLabel(): string {
        return match($this) {
            self::LOW => 'Low Priority',
            self::MEDIUM => 'Medium Priority',
            self::HIGH => 'High Priority',
            self::CRITICAL => 'Critical Priority',
        };
    }
}

/**
 * Anonymous class
 */
$anonymousClass = new class($config) extends BaseModel {
    private array $config;
    
    public function __construct(array $config) {
        $this->config = $config;
    }
    
    public function save(): bool {
        return true;
    }
    
    public function delete(): bool {
        return false;
    }
};

/**
 * Closure with use statement
 */
$multiplier = 10;
$closure = function(int $x) use ($multiplier): int {
    return $x * $multiplier;
};

/**
 * Static closure
 */
$staticClosure = static function(): void {
    echo "Static closure\n";
};

/**
 * Generator function
 */
function fibonacci(int $n): Generator {
    $a = 0;
    $b = 1;
    
    for ($i = 0; $i < $n; $i++) {
        yield $a;
        [$a, $b] = [$b, $a + $b];
    }
}

/**
 * Attributes (PHP 8.0+)
 */
#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_METHOD)]
class Route {
    public function __construct(
        public string $path,
        public string $method = 'GET'
    ) {}
}

#[Attribute(Attribute::TARGET_PROPERTY)]
class Inject {
    public function __construct(public string $service) {}
}

#[Attribute(Attribute::TARGET_PARAMETER)]
class Validate {
    public function __construct(public array $rules) {}
}

/**
 * Class using attributes
 */
#[Route('/users', 'GET')]
class UserController {
    #[Inject('database')]
    private $db;
    
    #[Route('/users/{id}', 'GET')]
    public function show(
        #[Validate(['required', 'integer'])] int $id
    ): User {
        return User::find($id);
    }
    
    #[Route('/users', 'POST')]
    public function store(array $data): User {
        return User::create($data);
    }
}

/**
 * Match expression (PHP 8.0+)
 */
function processValue(mixed $value): string {
    return match (gettype($value)) {
        'integer' => "Integer: $value",
        'double' => "Float: $value",
        'string' => "String: $value",
        'boolean' => $value ? 'True' : 'False',
        'array' => 'Array with ' . count($value) . ' elements',
        'object' => 'Object of class ' . get_class($value),
        default => 'Unknown type',
    };
}

/**
 * Null safe operator (PHP 8.0+)
 */
$result = $user?->getProfile()?->getAddress()?->getCity();

/**
 * Constructor property promotion with readonly (PHP 8.1+)
 */
class ReadonlyClass {
    public function __construct(
        public readonly string $id,
        public readonly DateTime $createdAt = new DateTime(),
        private readonly array $config = []
    ) {}
}

/**
 * First-class callable syntax (PHP 8.1+)
 */
$fn = strlen(...);
$method = $object->method(...);
$staticMethod = SomeClass::staticMethod(...);

/**
 * Never return type (PHP 8.1+)
 */
function terminate(string $message): never {
    throw new RuntimeException($message);
}

/**
 * Fibers (PHP 8.1+)
 */
$fiber = new Fiber(function(): void {
    $value = Fiber::suspend('suspended');
    echo "Resumed with: $value\n";
});

$suspended = $fiber->start();
$fiber->resume('resumed value');

/**
 * DNF Types - Disjunctive Normal Form (PHP 8.2+)
 */
function processCollection((Countable&Iterator)|array $data): int {
    if (is_array($data)) {
        return count($data);
    }
    return iterator_count($data);
}

/**
 * Constants in traits (PHP 8.2+)
 */
trait ConstantTrait {
    public const TRAIT_CONSTANT = 'value';
    final public const FINAL_CONSTANT = 'final';
}

/**
 * Dynamic class constant fetch (PHP 8.3+)
 */
$constantName = 'CLASS_CONSTANT';
$value = SimpleClass::{$constantName};

/**
 * Typed class constants (PHP 8.3+)
 */
class TypedConstants {
    public const string NAME = 'TypedConstants';
    public const int VERSION = 1;
    public const array OPTIONS = ['debug' => true];
}

// Exception handling
try {
    throw new Exception('Test exception');
} catch (RuntimeException $e) {
    echo "Runtime: {$e->getMessage()}\n";
} catch (Exception | Error $e) {
    echo "General: {$e->getMessage()}\n";
} finally {
    echo "Cleanup\n";
}

// Custom exception
class CustomException extends Exception {
    public function __construct(
        string $message = "",
        int $code = 0,
        ?Throwable $previous = null
    ) {
        parent::__construct($message, $code, $previous);
    }
}

// Namespace aliasing
namespace AliasExample {
    use App\Examples\Comprehensive\UserModel as User;
    use function App\Examples\Comprehensive\simpleFunction as calc;
    use const App\Examples\Comprehensive\MAX_SIZE as LIMIT;
}

// Global namespace
namespace {
    // Global code
    $globalVar = 'global';
    
    if (PHP_VERSION_ID >= 80000) {
        echo "PHP 8.0+ features available\n";
    }
}