// Example JavaScript test file for Hoya
// This file demonstrates the available APIs

// Test app_log API
app_log("INFO", "Testing app_log API");
app_log("ERROR", "This is an error message");
app_log("DEBUG", "This is a debug message");

// Test get_unixtime API
const timestamp = get_unixtime();
app_log("INFO", `Current Unix timestamp: ${timestamp}`);

// Test console.log (should be captured by app_log internally)
console.log("This is a console.log message");

// Try to use fetch API (will throw an error since it's not fully implemented)
try {
  fetch({ url: "https://example.com" });
} catch (error) {
  app_log("ERROR", `Fetch failed as expected: ${error.message}`);
}

// Return a value (will be sent back in the response)
("JavaScript execution completed successfully. 🎉");
