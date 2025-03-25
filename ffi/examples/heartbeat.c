// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/_types/_u_int64_t.h>

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
  IdeviceErrorCode err = idevice_pairing_file_read(
      "/Users/jacksoncoxson/Desktop/storage/00008140-001809302684801C.plist",
      &pairing_file);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  // Create TCP provider
  TcpProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "ExampleProvider", &provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing_file);
    return 1;
  }

  // Connect to installation proxy
  HeartbeatClientHandle *client = NULL;
  err = heartbeat_connect_tcp(provider, &client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to installation proxy: %d\n", err);
    tcp_provider_free(provider);
    return 1;
  }
  tcp_provider_free(provider);

  u_int64_t current_interval = 15;
  while (1) {
    // Get the new interval
    u_int64_t new_interval = 0;
    err = heartbeat_get_marco(client, current_interval, &new_interval);
    if (err != IdeviceSuccess) {
      fprintf(stderr, "Failed to get marco: %d\n", err);
      heartbeat_client_free(client);
      return 1;
    }
    current_interval = new_interval + 5;

    // Reply
    err = heartbeat_send_polo(client);
    if (err != IdeviceSuccess) {
      fprintf(stderr, "Failed to get marco: %d\n", err);
      heartbeat_client_free(client);
      return 1;
    }
  }
}
