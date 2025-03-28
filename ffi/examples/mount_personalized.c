#include "idevice.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// Helper function to read file contents
static uint8_t *read_file(const char *path, size_t *len) {
  FILE *file = fopen(path, "rb");
  if (!file) {
    perror("Failed to open file");
    return NULL;
  }

  fseek(file, 0, SEEK_END);
  long file_size = ftell(file);
  fseek(file, 0, SEEK_SET);

  uint8_t *buffer = malloc(file_size);
  if (!buffer) {
    fclose(file);
    return NULL;
  }

  if (fread(buffer, 1, file_size, file) != (size_t)file_size) {
    free(buffer);
    fclose(file);
    return NULL;
  }

  fclose(file);
  *len = file_size;
  return buffer;
}

// Callback function to show progress
void progress_callback(size_t progress, size_t total, void *context) {
  size_t percent = (progress * 100) / total;

  printf("\rUpload progress: %zu%%", percent);
  fflush(stdout);

  // Print newline when complete
  if (progress == total) {
    printf("\n");
  }
}

int main(int argc, char **argv) {
  if (argc < 5) {
    fprintf(stderr,
            "Usage: %s <device_ip> <image> <trustcache> <build_manifest> "
            "[pairing_file]\n",
            argv[0]);
    return 1;
  }

  const char *device_ip = argv[1];
  const char *image_path = argv[2];
  const char *trustcache_path = argv[3];
  const char *manifest_path = argv[4];
  const char *pairing_file_path = argc > 5 ? argv[5] : "pairing_file.plist";

  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  // Read files
  size_t image_len, trustcache_len, manifest_len;
  uint8_t *image = read_file(image_path, &image_len);
  uint8_t *trustcache = read_file(trustcache_path, &trustcache_len);
  uint8_t *build_manifest = read_file(manifest_path, &manifest_len);

  if (!image || !trustcache || !build_manifest) {
    fprintf(stderr, "Failed to read one or more files\n");
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Create the socket address
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, device_ip, &addr.sin_addr);

  // Read pairing file
  IdevicePairingFile *pairing_file = NULL;
  IdeviceErrorCode err =
      idevice_pairing_file_read(pairing_file_path, &pairing_file);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Create TCP provider
  TcpProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "ImageMounterTest", &provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing_file);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Read pairing file
  IdevicePairingFile *pairing_file_2 = NULL;
  err = idevice_pairing_file_read(pairing_file_path, &pairing_file_2);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Connect to lockdownd
  LockdowndClientHandle *lockdown_client = NULL;
  err = lockdownd_connect_tcp(provider, &lockdown_client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to lockdownd: %d\n", err);
    tcp_provider_free(provider);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Start session
  err = lockdownd_start_session(lockdown_client, pairing_file_2);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to start session: %d\n", err);
    lockdownd_client_free(lockdown_client);
    tcp_provider_free(provider);
    idevice_pairing_file_free(pairing_file_2);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }
  idevice_pairing_file_free(pairing_file_2);

  // Get UniqueChipID
  plist_t unique_chip_id_plist = NULL;
  err = lockdownd_get_value(lockdown_client, "UniqueChipID",
                            &unique_chip_id_plist);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get UniqueChipID: %d\n", err);
    lockdownd_client_free(lockdown_client);
    tcp_provider_free(provider);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  uint64_t unique_chip_id = 0;
  plist_get_uint_val(unique_chip_id_plist, &unique_chip_id);
  plist_free(unique_chip_id_plist);

  // Connect to image mounter
  ImageMounterHandle *mounter_client = NULL;
  err = image_mounter_connect_tcp(provider, &mounter_client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to image mounter: %d\n", err);
    lockdownd_client_free(lockdown_client);
    tcp_provider_free(provider);
    free(image);
    free(trustcache);
    free(build_manifest);
    return 1;
  }

  // Mount personalized image with progress callback
  err = image_mounter_mount_personalized_tcp_with_callback(
      mounter_client, provider, image, image_len, trustcache, trustcache_len,
      build_manifest, manifest_len,
      NULL, // info_plist
      unique_chip_id, progress_callback, NULL);

  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to mount personalized image: %d\n", err);
  } else {
    printf("Successfully mounted personalized image!\n");
  }

  // Cleanup
  image_mounter_free(mounter_client);
  lockdownd_client_free(lockdown_client);
  tcp_provider_free(provider);
  free(image);
  free(trustcache);
  free(build_manifest);

  return err == IdeviceSuccess ? 0 : 1;
}
