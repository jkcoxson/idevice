// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main() {
  // Create the socket address (IPv4 example)
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, "10.7.0.2", &addr.sin_addr); // Replace with actual IP

  // Allocate device handle
  IdeviceHandle *idevice = NULL;

  // Call the Rust function to connect
  IdeviceFfiError *err = idevice_new_tcp_socket(
      (struct sockaddr *)&addr, sizeof(addr), "TestDevice", &idevice);

  if (err != NULL) {
    fprintf(stderr, "Failed to connect to device: [%d] %s\n", err->code,
            err->message);
    idevice_error_free(err);
    return 1;
  }

  printf("Connected to device successfully!\n");

  // Get device type
  char *device_type = NULL;
  err = idevice_get_type(idevice, &device_type);

  if (err != NULL) {
    fprintf(stderr, "Failed to get device type: [%d] %s\n", err->code,
            err->message);
    idevice_error_free(err);
    idevice_free(idevice);
    return 1;
  }

  printf("Service Type: %s\n", device_type);

  // Free the string
  idevice_string_free(device_type);

  // Close the device connection
  idevice_free(idevice);

  return 0;
}
