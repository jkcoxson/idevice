// Jackson Coxson

#include "idevice.h"
#include "plist/plist.h"
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void print_usage() {
  printf("Usage: image_mounter_test [options] [command]\n");
  printf("Options:\n");
  printf("  --ip IP_ADDRESS       Device IP address (default: 10.7.0.2)\n");
  printf("  --pairing FILE        Pairing file path (default: "
         "pairing_file.plist)\n");
  printf("\nCommands:\n");
  printf("  list-devices          List mounted devices\n");
  printf("  lookup TYPE           Lookup image signature by type\n");
  printf("  upload TYPE IMG SIG   Upload an image with signature\n");
  printf("  mount TYPE SIG [TC]   Mount an image (optional trust cache)\n");
  printf("  unmount PATH          Unmount image at path\n");
  printf("  dev-status            Query developer mode status\n");
  printf("  mount-dev IMG SIG     Mount developer image\n");
  printf("  help                  Show this help message\n");
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
  char *command = NULL;

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
    } else if (strcmp(argv[i], "help") == 0) {
      print_usage();
      return 0;
    } else {
      command = argv[i];
      break;
    }
  }

  if (!command) {
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
  IdeviceErrorCode err =
      idevice_pairing_file_read(pairing_file_path, &pairing_file);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  // Create TCP provider
  IdeviceProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "ImageMounterTest", &provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing_file);
    return 1;
  }

  // Connect to image mounter
  ImageMounterHandle *client = NULL;
  err = image_mounter_connect(provider, &client);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to image mounter: %d\n", err);
    idevice_provider_free(provider);
    return 1;
  }
  idevice_provider_free(provider);

  // Process command
  int success = 1;
  if (strcmp(command, "list-devices") == 0) {
    void *devices = NULL;
    size_t devices_len = 0;
    err = image_mounter_copy_devices(client, &devices, &devices_len);
    if (err == IdeviceSuccess) {
      plist_t *device_list = (plist_t *)devices;
      printf("Mounted devices:\n");
      for (size_t i = 0; i < devices_len; i++) {
        plist_t device = device_list[i];
        char *xml = NULL;
        uint32_t len = 0;
        plist_to_xml(device, &xml, &len);
        printf("- %s\n", xml);
        plist_mem_free(xml);
        plist_free(device);
      }
    } else {
      fprintf(stderr, "Failed to get device list: %d\n", err);
      success = 0;
    }
  } else if (strcmp(command, "lookup") == 0) {
    if (argc < 3) {
      printf("Error: Missing image type argument\n");
      success = 0;
    } else {
      char *image_type = argv[2];
      uint8_t *signature = NULL;
      size_t signature_len = 0;

      err = image_mounter_lookup_image(client, image_type, &signature,
                                       &signature_len);
      if (err == IdeviceSuccess) {
        printf("Signature for %s (%zu bytes):\n", image_type, signature_len);
        for (size_t i = 0; i < signature_len; i++) {
          printf("%02x", signature[i]);
        }
        printf("\n");
        free(signature);
      } else {
        fprintf(stderr, "Failed to lookup image: %d\n", err);
        success = 0;
      }
    }
  } else if (strcmp(command, "upload") == 0) {
    if (argc < 5) {
      printf("Error: Missing arguments for upload\n");
      success = 0;
    } else {
      char *image_type = argv[2];
      char *image_file = argv[3];
      char *signature_file = argv[4];

      uint8_t *image_data = NULL;
      size_t image_len = 0;
      uint8_t *signature_data = NULL;
      size_t signature_len = 0;

      if (!read_file(image_file, &image_data, &image_len)) {
        success = 0;
      } else if (!read_file(signature_file, &signature_data, &signature_len)) {
        free(image_data);
        success = 0;
      } else {
        err = image_mounter_upload_image(client, image_type, image_data,
                                         image_len, signature_data,
                                         signature_len);
        if (err == IdeviceSuccess) {
          printf("Image uploaded successfully\n");
        } else {
          fprintf(stderr, "Failed to upload image: %d\n", err);
          success = 0;
        }

        free(image_data);
        free(signature_data);
      }
    }
  } else if (strcmp(command, "mount") == 0) {
    if (argc < 4) {
      printf("Error: Missing arguments for mount\n");
      success = 0;
    } else {
      char *image_type = argv[2];
      char *signature_file = argv[3];
      char *trust_cache_file = (argc > 4) ? argv[4] : NULL;

      uint8_t *signature_data = NULL;
      size_t signature_len = 0;
      uint8_t *trust_cache_data = NULL;
      size_t trust_cache_len = 0;

      if (!read_file(signature_file, &signature_data, &signature_len)) {
        success = 0;
      } else {
        if (trust_cache_file &&
            !read_file(trust_cache_file, &trust_cache_data, &trust_cache_len)) {
          free(signature_data);
          success = 0;
        } else {
          err = image_mounter_mount_image(
              client, image_type, signature_data, signature_len,
              trust_cache_data, trust_cache_len,
              NULL); // No info plist in this example
          if (err == IdeviceSuccess) {
            printf("Image mounted successfully\n");
          } else {
            fprintf(stderr, "Failed to mount image: %d\n", err);
            success = 0;
          }

          free(signature_data);
          if (trust_cache_data)
            free(trust_cache_data);
        }
      }
    }
  } else if (strcmp(command, "unmount") == 0) {
    if (argc < 3) {
      printf("Error: Missing mount path argument\n");
      success = 0;
    } else {
      char *mount_path = argv[2];
      err = image_mounter_unmount_image(client, mount_path);
      if (err == IdeviceSuccess) {
        printf("Image unmounted successfully\n");
      } else {
        fprintf(stderr, "Failed to unmount image: %d\n", err);
        success = 0;
      }
    }
  } else if (strcmp(command, "dev-status") == 0) {
    int status = 0;
    err = image_mounter_query_developer_mode_status(client, &status);
    if (err == IdeviceSuccess) {
      printf("Developer mode status: %s\n", status ? "enabled" : "disabled");
    } else {
      fprintf(stderr, "Failed to query developer mode status: %d\n", err);
      success = 0;
    }
  } else if (strcmp(command, "mount-dev") == 0) {
    if (argc < 4) {
      printf("Error: Missing arguments for mount-dev\n");
      success = 0;
    } else {
      char *image_file = argv[2];
      char *signature_file = argv[3];

      uint8_t *image_data = NULL;
      size_t image_len = 0;
      uint8_t *signature_data = NULL;
      size_t signature_len = 0;

      if (!read_file(image_file, &image_data, &image_len)) {
        success = 0;
      } else if (!read_file(signature_file, &signature_data, &signature_len)) {
        free(image_data);
        success = 0;
      } else {
        err = image_mounter_mount_developer(client, image_data, image_len,
                                            signature_data, signature_len);
        if (err == IdeviceSuccess) {
          printf("Developer image mounted successfully\n");
        } else {
          fprintf(stderr, "Failed to mount developer image: %d\n", err);
          success = 0;
        }

        free(image_data);
        free(signature_data);
      }
    }
  } else {
    printf("Unknown command: %s\n", command);
    print_usage();
    success = 0;
  }

  // Cleanup
  image_mounter_free(client);

  return success ? 0 : 1;
}
