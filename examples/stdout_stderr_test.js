// Test file for stdout and stderr capturing
// This file outputs to both stdout (via console.log) and stderr (via console.error)

// Regular output
console.log("This is a standard output message");
console.log("This is another standard output message");

// Object output
console.log("Object output:", { name: "Test Object", value: 42 });

// Error output
console.error("This is an error message");
console.error("This is another error message");

// Multiple arguments
console.log("Multiple", "arguments", "test", 123);

// Using app_log function (which should also be captured)
app_log("INFO", "This is a log message via app_log");
app_log("ERROR", "This is an error message via app_log");

// Result value returned from the script
("Execution completed successfully!");
