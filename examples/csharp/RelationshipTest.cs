/// <summary>
/// Relationship and Call Graph Test File for Codanna.
///
/// This file is specifically designed to test:
/// 1. Method call tracking (get_calls MCP tool)
/// 2. Caller detection (find_callers MCP tool)
/// 3. Impact analysis (analyze_impact MCP tool)
/// 4. Cross-class method calls
/// 5. Interface method resolution
///
/// Expected Relationships:
/// - ServiceOrchestrator.Execute() CALLS:
///   - DataService.FetchData()
///   - ValidationService.Validate()
///   - ProcessingService.Process()
///   - NotificationService.Notify()
///
/// - DataService.FetchData() CALLS:
///   - DataService.ConnectToDatabase()
///   - DataService.QueryDatabase()
///   - DataService.CloseConnection()
///
/// - ProcessingService.Process() CALLS:
///   - ProcessingService.Transform()
///   - ProcessingService.ApplyRules()
///   - ProcessingService.Validate()
/// </summary>

using System;
using System.Collections.Generic;

namespace Codanna.Examples.Relationships
{
    // === SIMPLE CALL CHAIN: A -> B -> C ===

    /// <summary>
    /// Service A - Top of the call chain.
    /// Tests: Simple forward call tracking
    /// </summary>
    public class ServiceA
    {
        private readonly ServiceB _serviceB;

        public ServiceA(ServiceB serviceB)
        {
            _serviceB = serviceB;
        }

        /// <summary>
        /// Method that starts the call chain.
        /// CALLS: ServiceB.MethodB()
        /// </summary>
        public string MethodA(string input)
        {
            Console.WriteLine("ServiceA.MethodA called");

            // This call should show up in MCP get_calls ServiceA.MethodA
            return _serviceB.MethodB(input);
        }
    }

    /// <summary>
    /// Service B - Middle of the call chain.
    /// Tests: Method is both caller and callee
    /// </summary>
    public class ServiceB
    {
        private readonly ServiceC _serviceC;

        public ServiceB(ServiceC serviceC)
        {
            _serviceC = serviceC;
        }

        /// <summary>
        /// Method in the middle of the chain.
        /// CALLED BY: ServiceA.MethodA()
        /// CALLS: ServiceC.MethodC()
        /// </summary>
        public string MethodB(string input)
        {
            Console.WriteLine("ServiceB.MethodB called");

            // This call should show up in MCP get_calls ServiceB.MethodB
            return _serviceC.MethodC(input);
        }
    }

    /// <summary>
    /// Service C - End of the call chain.
    /// Tests: Method is only callee, no further calls
    /// </summary>
    public class ServiceC
    {
        /// <summary>
        /// Method at the end of the chain.
        /// CALLED BY: ServiceB.MethodB()
        /// CALLS: Nothing (leaf method)
        /// </summary>
        public string MethodC(string input)
        {
            Console.WriteLine("ServiceC.MethodC called");
            return input.ToUpper();
        }
    }

    // === MULTIPLE CALLERS: Many -> One ===

    /// <summary>
    /// Shared utility service called by multiple services.
    /// Tests: find_callers should show multiple callers
    /// </summary>
    public class LoggerService
    {
        /// <summary>
        /// Log method called by many services.
        /// CALLED BY: ServiceOrchestrator.Execute(), DataService.FetchData(), ValidationService.Validate()
        /// </summary>
        public void Log(string message)
        {
            Console.WriteLine($"[LOG] {message}");
        }
    }

    // === ORCHESTRATOR PATTERN: One -> Many ===

    /// <summary>
    /// Orchestrator that calls multiple services.
    /// Tests: get_calls should show multiple callees
    /// </summary>
    public class ServiceOrchestrator
    {
        private readonly DataService _dataService;
        private readonly ValidationService _validationService;
        private readonly ProcessingService _processingService;
        private readonly NotificationService _notificationService;
        private readonly LoggerService _logger;

        public ServiceOrchestrator(
            DataService dataService,
            ValidationService validationService,
            ProcessingService processingService,
            NotificationService notificationService,
            LoggerService logger)
        {
            _dataService = dataService;
            _validationService = validationService;
            _processingService = processingService;
            _notificationService = notificationService;
            _logger = logger;
        }

        /// <summary>
        /// Orchestrates multiple service calls.
        /// CALLS: 5 different methods
        /// Tests: Multiple outbound calls from one method
        /// </summary>
        public void Execute(string request)
        {
            // Call 1: LoggerService.Log
            _logger.Log("Starting execution");

            // Call 2: DataService.FetchData
            var data = _dataService.FetchData(request);

            // Call 3: ValidationService.Validate
            if (!_validationService.Validate(data))
            {
                _logger.Log("Validation failed");
                return;
            }

            // Call 4: ProcessingService.Process
            var result = _processingService.Process(data);

            // Call 5: NotificationService.Notify
            _notificationService.Notify("Processing completed", result);

            _logger.Log("Execution completed");
        }
    }

    // === SUPPORTING SERVICES ===

    /// <summary>
    /// Data service with internal call chain.
    /// Tests: Internal method calls within same class
    /// </summary>
    public class DataService
    {
        private readonly LoggerService _logger;

        public DataService(LoggerService logger)
        {
            _logger = logger;
        }

        /// <summary>
        /// Fetch data with internal call chain.
        /// CALLED BY: ServiceOrchestrator.Execute()
        /// CALLS: ConnectToDatabase(), QueryDatabase(), CloseConnection()
        /// </summary>
        public string FetchData(string request)
        {
            _logger.Log("Fetching data");

            // Internal calls (same class)
            ConnectToDatabase();
            var data = QueryDatabase(request);
            CloseConnection();

            return data;
        }

        /// <summary>
        /// Internal method - connect to database.
        /// CALLED BY: FetchData()
        /// </summary>
        private void ConnectToDatabase()
        {
            Console.WriteLine("Connecting to database...");
        }

        /// <summary>
        /// Internal method - query database.
        /// CALLED BY: FetchData()
        /// </summary>
        private string QueryDatabase(string request)
        {
            Console.WriteLine($"Querying database for: {request}");
            return $"Data for {request}";
        }

        /// <summary>
        /// Internal method - close connection.
        /// CALLED BY: FetchData()
        /// </summary>
        private void CloseConnection()
        {
            Console.WriteLine("Closing database connection...");
        }
    }

    /// <summary>
    /// Validation service.
    /// Tests: Simple single call
    /// </summary>
    public class ValidationService
    {
        private readonly LoggerService _logger;

        public ValidationService(LoggerService logger)
        {
            _logger = logger;
        }

        /// <summary>
        /// Validate data.
        /// CALLED BY: ServiceOrchestrator.Execute()
        /// CALLS: LoggerService.Log()
        /// </summary>
        public bool Validate(string data)
        {
            _logger.Log("Validating data");
            return !string.IsNullOrEmpty(data);
        }
    }

    /// <summary>
    /// Processing service with internal pipeline.
    /// Tests: Sequential internal calls
    /// </summary>
    public class ProcessingService
    {
        /// <summary>
        /// Process data through pipeline.
        /// CALLED BY: ServiceOrchestrator.Execute()
        /// CALLS: Transform(), ApplyRules(), ValidateResult()
        /// </summary>
        public string Process(string data)
        {
            // Processing pipeline (sequential calls)
            data = Transform(data);
            data = ApplyRules(data);
            ValidateResult(data);

            return data;
        }

        /// <summary>
        /// Transform data.
        /// CALLED BY: Process()
        /// </summary>
        private string Transform(string data)
        {
            return data.ToUpper();
        }

        /// <summary>
        /// Apply business rules.
        /// CALLED BY: Process()
        /// </summary>
        private string ApplyRules(string data)
        {
            return $"[PROCESSED] {data}";
        }

        /// <summary>
        /// Validate the result.
        /// CALLED BY: Process()
        /// </summary>
        private void ValidateResult(string data)
        {
            if (string.IsNullOrEmpty(data))
                throw new InvalidOperationException("Processing failed");
        }
    }

    /// <summary>
    /// Notification service.
    /// Tests: Leaf service with no outbound calls
    /// </summary>
    public class NotificationService
    {
        /// <summary>
        /// Send notification.
        /// CALLED BY: ServiceOrchestrator.Execute()
        /// CALLS: Nothing (leaf method)
        /// </summary>
        public void Notify(string title, string message)
        {
            Console.WriteLine($"NOTIFICATION: {title} - {message}");
        }
    }

    // === INTERFACE IMPLEMENTATION CALLS ===

    /// <summary>
    /// Repository interface.
    /// Tests: Interface method calls
    /// </summary>
    public interface IRepository
    {
        string GetById(int id);
        void Save(string data);
    }

    /// <summary>
    /// Repository implementation.
    /// Tests: Interface method implementation
    /// </summary>
    public class DatabaseRepository : IRepository
    {
        /// <inheritdoc />
        public string GetById(int id)
        {
            return $"Data for ID {id}";
        }

        /// <inheritdoc />
        public void Save(string data)
        {
            Console.WriteLine($"Saving: {data}");
        }
    }

    /// <summary>
    /// Service using repository interface.
    /// Tests: Calls to interface methods
    /// </summary>
    public class RepositoryConsumer
    {
        private readonly IRepository _repository;

        public RepositoryConsumer(IRepository repository)
        {
            _repository = repository;
        }

        /// <summary>
        /// Load and save data.
        /// CALLS: IRepository.GetById(), IRepository.Save()
        /// </summary>
        public void LoadAndSave(int id)
        {
            // Call to interface method
            var data = _repository.GetById(id);

            // Another call to interface method
            _repository.Save(data + " [modified]");
        }
    }

    // === RECURSIVE CALLS ===

    /// <summary>
    /// Service with recursive method.
    /// Tests: Self-referential calls
    /// </summary>
    public class RecursiveService
    {
        /// <summary>
        /// Calculate factorial recursively.
        /// CALLS: Itself (RecursiveService.Factorial)
        /// </summary>
        public int Factorial(int n)
        {
            if (n <= 1)
                return 1;

            // Recursive call
            return n * Factorial(n - 1);
        }

        /// <summary>
        /// Count down with mutual recursion.
        /// CALLS: CountDownB()
        /// </summary>
        public void CountDownA(int n)
        {
            if (n > 0)
            {
                Console.WriteLine($"CountDownA: {n}");
                CountDownB(n - 1);
            }
        }

        /// <summary>
        /// Count down with mutual recursion.
        /// CALLS: CountDownA()
        /// </summary>
        private void CountDownB(int n)
        {
            if (n > 0)
            {
                Console.WriteLine($"CountDownB: {n}");
                CountDownA(n - 1);
            }
        }
    }

    // === STATIC METHOD CALLS ===

    /// <summary>
    /// Utility class with static methods.
    /// Tests: Static method calls
    /// </summary>
    public static class MathUtils
    {
        /// <summary>
        /// Add two numbers.
        /// CALLED BY: Calculator.Calculate()
        /// </summary>
        public static int Add(int a, int b)
        {
            return a + b;
        }

        /// <summary>
        /// Multiply two numbers.
        /// CALLED BY: Calculator.Calculate()
        /// </summary>
        public static int Multiply(int a, int b)
        {
            return a * b;
        }
    }

    /// <summary>
    /// Calculator using static utility methods.
    /// Tests: Calls to static methods
    /// </summary>
    public class Calculator
    {
        /// <summary>
        /// Perform calculation using static utilities.
        /// CALLS: MathUtils.Add(), MathUtils.Multiply()
        /// </summary>
        public int Calculate(int a, int b, int c)
        {
            // Call static method
            var sum = MathUtils.Add(a, b);

            // Call another static method
            return MathUtils.Multiply(sum, c);
        }
    }
}
