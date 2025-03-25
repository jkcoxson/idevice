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
  InstallationProxyClientHandle *client = NULL;
  err = installation_proxy_connect_tcp(provider, &client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to installation proxy: %d\n", err);
    tcp_provider_free(provider);
    return 1;
  }

  // Get all apps (pass NULL for both filters to get everything)
  void *apps = NULL;
  size_t apps_len = 0;
  err = installation_proxy_get_apps(client,
                                    NULL, // application_type filter
                                    NULL, // bundle_identifiers filter
                                    0,    // bundle_identifiers length
                                    &apps, &apps_len);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get apps: %d\n", err);
    installation_proxy_client_free(client);
    tcp_provider_free(provider);
    return 1;
  }

  // Cast the result to plist_t array
  plist_t *app_list = (plist_t *)apps;

  printf("Found %zu apps:\n", apps_len);
  for (size_t i = 0; i < apps_len; i++) {
    plist_t app = app_list[i];

    // Get CFBundleIdentifier (you'd need proper plist dict access here)
    plist_t bundle_id_node = plist_dict_get_item(app, "CFBundleIdentifier");
    if (bundle_id_node) {
      char *bundle_id = NULL;
      plist_get_string_val(bundle_id_node, &bundle_id);
      printf("- %s\n", bundle_id);
      free(bundle_id);
    }
  }

  // Cleanup
  installation_proxy_client_free(client);
  tcp_provider_free(provider);

  return 0;
}
