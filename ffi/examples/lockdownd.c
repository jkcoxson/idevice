// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main() {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  // Create the socket address (replace with your device's IP)
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, "10.7.0.2", &addr.sin_addr);

  // Read pairing file (replace with your pairing file path)
  IdevicePairingFile *pairing_file = NULL;
  IdeviceErrorCode err =
      idevice_pairing_file_read("pairing_file.plist", &pairing_file);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  // Create TCP provider
  TcpProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "LockdowndTest", &provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing_file);
    return 1;
  }

  // Connect to lockdownd
  LockdowndClientHandle *client = NULL;
  err = lockdownd_connect_tcp(provider, &client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to lockdownd: %d\n", err);
    tcp_provider_free(provider);
    return 1;
  }

  // Read pairing file (replace with your pairing file path)
  IdevicePairingFile *pairing_file_2 = NULL;
  err = idevice_pairing_file_read("pairing_file.plist", &pairing_file_2);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  // Start session
  err = lockdownd_start_session(client, pairing_file_2);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to start session: %d\n", err);
    lockdownd_client_free(client);
    tcp_provider_free(provider);
    return 1;
  }

  // Get device name
  plist_t name_plist = NULL;
  err = lockdownd_get_value(client, "DeviceName", &name_plist);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get device name: %d\n", err);
  } else {
    char *name = NULL;
    plist_get_string_val(name_plist, &name);
    printf("Device name: %s\n", name);
    free(name);
    plist_free(name_plist);
  }

  // Get product version
  plist_t version_plist = NULL;
  err = lockdownd_get_value(client, "ProductVersion", &version_plist);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get product version: %d\n", err);
  } else {
    char *version = NULL;
    plist_get_string_val(version_plist, &version);
    printf("iOS version: %s\n", version);
    free(version);
    plist_free(version_plist);
  }

  // Get all values
  plist_t all_values = NULL;
  err = lockdownd_get_all_values(client, &all_values);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get all values: %d\n", err);
  } else {
    printf("\nAll device values:\n");
    // Iterate through dictionary (simplified example)
    plist_dict_iter it = NULL;
    plist_dict_new_iter(all_values, &it);
    if (it) {
      char *key = NULL;
      plist_t val = NULL;
      do {
        plist_dict_next_item(all_values, it, &key, &val);
        if (key) {
          printf("- %s: ", key);
          // Print value based on type (simplified)
          if (plist_get_node_type(val) == PLIST_STRING) {
            char *str_val = NULL;
            plist_get_string_val(val, &str_val);
            printf("%s", str_val);
            free(str_val);
          } else if (plist_get_node_type(val) == PLIST_BOOLEAN) {
            uint8_t bool_val = 0;
            plist_get_bool_val(val, &bool_val);
            printf("%s", bool_val ? "true" : "false");
          } else if (plist_get_node_type(val) == PLIST_UINT) {
            uint64_t int_val = 0;
            plist_get_uint_val(val, &int_val);
            printf("%llu", int_val);
          }
          printf("\n");
          free(key);
        }
      } while (key);
      free(it);
    }
    plist_free(all_values);
  }

  // Test starting a service (heartbeat in this example)
  uint16_t port = 0;
  bool ssl = false;
  err = lockdownd_start_service(client, "com.apple.mobile.heartbeat", &port,
                                &ssl);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to start heartbeat service: %d\n", err);
  } else {
    printf("\nStarted heartbeat service on port %d (SSL: %s)\n", port,
           ssl ? "true" : "false");
  }

  // Cleanup
  lockdownd_client_free(client);
  tcp_provider_free(provider);

  return 0;
}
