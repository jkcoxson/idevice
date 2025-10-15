#include "idevice.h"
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void print_usage() {
  printf("Usage: ipa_installer [options] <ipa_path>\n");
  printf("Options:\n");
  printf("  --ip IP_ADDRESS       Device IP address (default: 10.7.0.2)\n");
  printf("  --pairing FILE        Pairing file path (default: "
         "pairing_file.plist)\n");
  printf("  --udid UDID           Device UDID (optional)\n");
}

int read_file(const char *filename, uint8_t **data, size_t *length) {
  FILE *file = fopen(filename, "rb");
  if (!file) {
    perror("Failed to open file");
    return 0;
  }

  fseek(file, 0, SEEK_END);
  *length = ftell(file);
  fseek(file, 0, SEEK_SET);

  *data = malloc(*length);
  if (!*data) {
    perror("Failed to allocate memory");
    fclose(file);
    return 0;
  }

  if (fread(*data, 1, *length, file) != *length) {
    perror("Failed to read file");
    free(*data);
    fclose(file);
    return 0;
  }

  fclose(file);
  return 1;
}

int main(int argc, char **argv) {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  // Default values
  char *ip = "10.7.0.2";
  char *pairing_file_path = "pairing_file.plist";
  char *udid = NULL;
  char *ipa_path = NULL;

  // Parse arguments
  for (int i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--ip") == 0) {
      if (i + 1 >= argc) {
        printf("Error: Missing IP address argument\n");
        return 1;
      }
      ip = argv[++i];
    } else if (strcmp(argv[i], "--pairing") == 0) {
      if (i + 1 >= argc) {
        printf("Error: Missing pairing file argument\n");
        return 1;
      }
      pairing_file_path = argv[++i];
    } else if (strcmp(argv[i], "--udid") == 0) {
      if (i + 1 >= argc) {
        printf("Error: Missing UDID argument\n");
        return 1;
      }
      udid = argv[++i];
    } else if (strcmp(argv[i], "help") == 0) {
      print_usage();
      return 0;
    } else {
      ipa_path = argv[i];
      break;
    }
  }

  if (!ipa_path) {
    print_usage();
    return 1;
  }

  // Create the socket address
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  if (inet_pton(AF_INET, ip, &addr.sin_addr) != 1) {
    fprintf(stderr, "Invalid IP address\n");
    return 1;
  }

  // Read pairing file
  IdevicePairingFile *pairing_file = NULL;
  IdeviceFfiError *err =
      idevice_pairing_file_read(pairing_file_path, &pairing_file);
  if (err != NULL) {
    fprintf(stderr, "Failed to read pairing file: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    return 1;
  }

  // Create TCP provider
  IdeviceProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "IPAInstaller", &provider);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP provider: [%d] %s", err->code,
            err->message);
    idevice_pairing_file_free(pairing_file);
    idevice_error_free(err);
    return 1;
  }

  // Connect to AFC service
  AfcClientHandle *afc_client = NULL;
  err = afc_client_connect(provider, &afc_client);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to AFC service: [%d] %s", err->code,
            err->message);
    idevice_provider_free(provider);
    idevice_error_free(err);
    return 1;
  }

  // Extract filename from path
  char *filename = strrchr(ipa_path, '/');
  if (filename == NULL) {
    filename = ipa_path;
  } else {
    filename++; // Skip the '/'
  }

  // Create destination path
  char dest_path[256];
  snprintf(dest_path, sizeof(dest_path), "/PublicStaging/%s", filename);

  // Upload IPA file
  printf("Uploading %s to %s...\n", ipa_path, dest_path);
  uint8_t *data = NULL;
  size_t length = 0;
  if (!read_file(ipa_path, &data, &length)) {
    fprintf(stderr, "Failed to read IPA file\n");
    afc_client_free(afc_client);
    return 1;
  }

  AfcFileHandle *file = NULL;
  err = afc_file_open(afc_client, dest_path, AfcWrOnly, &file);
  if (err != NULL) {
    fprintf(stderr, "Failed to open destination file: [%d] %s", err->code,
            err->message);
    free(data);
    afc_client_free(afc_client);
    idevice_error_free(err);
    return 1;
  }

  err = afc_file_write(file, data, length);
  free(data);
  afc_file_close(file);

  if (err != NULL) {
    fprintf(stderr, "Failed to write file: [%d] %s", err->code, err->message);
    afc_client_free(afc_client);
    idevice_error_free(err);
    return 1;
  }
  printf("Upload completed successfully\n");

  // Connect to installation proxy
  InstallationProxyClientHandle *instproxy_client = NULL;
  err = installation_proxy_connect(provider, &instproxy_client);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to installation proxy: [%d] %s",
            err->code, err->message);
    afc_client_free(afc_client);
    idevice_error_free(err);
    return 1;
  }

  // Install the uploaded IPA
  printf("Installing %s...\n", dest_path);
  err = installation_proxy_install(instproxy_client, dest_path, NULL);
  if (err != NULL) {
    fprintf(stderr, "Failed to install IPA: [%d] %s", err->code, err->message);
    idevice_error_free(err);
  } else {
    printf("Installation completed successfully\n");
  }

  // Cleanup
  installation_proxy_client_free(instproxy_client);
  afc_client_free(afc_client);
  idevice_provider_free(provider);

  return err == NULL ? 0 : 1;
}
