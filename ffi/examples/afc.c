// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void print_usage() {
  printf("Usage: afc_tool [options] [command] [args]\n");
  printf("Options:\n");
  printf("  --ip IP_ADDRESS       Device IP address (default: 10.7.0.2)\n");
  printf("  --pairing FILE        Pairing file path (default: "
         "pairing_file.plist)\n");
  printf("  --udid UDID           Device UDID (optional)\n");
  printf("\nCommands:\n");
  printf("  list PATH             List directory contents\n");
  printf("  mkdir PATH            Create directory\n");
  printf("  download SRC DEST     Download file from device\n");
  printf("  upload SRC DEST       Upload file to device\n");
  printf("  remove PATH           Remove file or directory\n");
  printf("  remove_all PATH       Recursively remove directory\n");
  printf("  info PATH             Get file information\n");
  printf("  device_info           Get device filesystem information\n");
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

int write_file(const char *filename, const uint8_t *data, size_t length) {
  FILE *file = fopen(filename, "wb");
  if (!file) {
    perror("Failed to open file");
    return 0;
  }

  if (fwrite(data, 1, length, file) != length) {
    perror("Failed to write file");
    fclose(file);
    return 0;
  }

  fclose(file);
  return 1;
}

void print_file_info(const AfcFileInfo *info) {
  printf("File Information:\n");
  printf("  Size: %zu bytes\n", info->size);
  printf("  Blocks: %zu\n", info->blocks);
  printf("  Created: %lld\n", (long long)info->creation);
  printf("  Modified: %lld\n", (long long)info->modified);
  printf("  Links: %s\n", info->st_nlink);
  printf("  Type: %s\n", info->st_ifmt);
  if (info->st_link_target) {
    printf("  Link Target: %s\n", info->st_link_target);
  }
}

void print_device_info(const AfcDeviceInfo *info) {
  printf("Device Information:\n");
  printf("  Model: %s\n", info->model);
  printf("  Total Space: %zu bytes\n", info->total_bytes);
  printf("  Free Space: %zu bytes\n", info->free_bytes);
  printf("  Block Size: %zu bytes\n", info->block_size);
}

void free_directory_listing(char **entries, size_t count) {
  if (!entries)
    return;
  for (size_t i = 0; i < count; i++) {
    if (entries[i]) {
      free(entries[i]);
    }
  }
  free(entries);
}

int main(int argc, char **argv) {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  // Default values
  char *ip = "10.7.0.2";
  char *pairing_file_path = "pairing_file.plist";
  char *udid = NULL;
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
  IdeviceFfiError *err =
      idevice_pairing_file_read(pairing_file_path, &pairing_file);
  if (err != NULL) {
    fprintf(stderr, "Failed to read pairing file: [%d] %s\n", err->code,
            err->message);
    idevice_error_free(err);
    return 1;
  }

  // Create TCP provider
  IdeviceProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "ImageMounterTest", &provider);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP provider: [%d] %s\n", err->code,
            err->message);
    idevice_error_free(err);
    idevice_pairing_file_free(pairing_file);
    return 1;
  }

  // Connect to AFC service
  AfcClientHandle *client = NULL;
  err = afc_client_connect(provider, &client);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to AFC service: [%d] %s\n", err->code,
            err->message);
    idevice_error_free(err);
    idevice_provider_free(provider);
    return 1;
  }
  idevice_provider_free(provider);

  // Process command
  int success = 1;

  if (strcmp(command, "list") == 0) {
    if (argc < 3) {
      printf("Error: Missing path argument\n");
      success = 0;
    } else {
      char *path = argv[2];
      char **entries = NULL;
      size_t count = 0;

      err = afc_list_directory(client, path, &entries, &count);
      if (err == NULL) {
        printf("Contents of %s:\n", path);
        for (size_t i = 0; i < count; i++) {
          printf("- %s\n", entries[i]);
        }
        free_directory_listing(entries, count);
      } else {
        fprintf(stderr, "Failed to list directory: [%d] %s\n", err->code,
                err->message);
        idevice_error_free(err);
        success = 0;
      }
    }
  } else if (strcmp(command, "mkdir") == 0) {
    if (argc < 3) {
      printf("Error: Missing path argument\n");
      success = 0;
    } else {
      char *path = argv[2];
      err = afc_make_directory(client, path);
      if (err == NULL) {
        printf("Directory created successfully\n");
      } else {
        fprintf(stderr, "Failed to create directory: [%d] %s\n", err->code,
                err->message);
        idevice_error_free(err);
        success = 0;
      }
    }
  } else if (strcmp(command, "download") == 0) {
    if (argc < 4) {
      printf("Error: Missing source and destination arguments\n");
      success = 0;
    } else {
      char *src_path = argv[2];
      char *dest_path = argv[3];

      AfcFileHandle *file = NULL;
      err = afc_file_open(client, src_path, AfcRdOnly, &file);
      if (err != NULL) {
        fprintf(stderr, "Failed to open file: [%d] %s\n", err->code,
                err->message);
        idevice_error_free(err);
        success = 0;
      } else {
        uint8_t *data = NULL;
        size_t length = 0;
        err = afc_file_read(file, &data, &length);
        if (err == NULL) {
          if (write_file(dest_path, data, length)) {
            printf("File downloaded successfully\n");
          } else {
            success = 0;
          }
          free(data);
        } else {
          fprintf(stderr, "Failed to read file: [%d] %s\n", err->code,
                  err->message);
          idevice_error_free(err);
          success = 0;
        }
        afc_file_close(file);
      }
    }
  } else if (strcmp(command, "upload") == 0) {
    if (argc < 4) {
      printf("Error: Missing source and destination arguments\n");
      success = 0;
    } else {
      char *src_path = argv[2];
      char *dest_path = argv[3];

      uint8_t *data = NULL;
      size_t length = 0;
      if (!read_file(src_path, &data, &length)) {
        success = 0;
      } else {
        AfcFileHandle *file = NULL;
        err = afc_file_open(client, dest_path, AfcWrOnly, &file);
        if (err != NULL) {
          fprintf(stderr, "Failed to open file: [%d] %s\n", err->code,
                  err->message);
          idevice_error_free(err);
          success = 0;
        } else {
          err = afc_file_write(file, data, length);
          if (err == NULL) {
            printf("File uploaded successfully\n");
          } else {
            fprintf(stderr, "Failed to write file: [%d] %s\n", err->code,
                    err->message);
            idevice_error_free(err);
            success = 0;
          }
          afc_file_close(file);
        }
        free(data);
      }
    }
  } else if (strcmp(command, "remove") == 0) {
    if (argc < 3) {
      printf("Error: Missing path argument\n");
      success = 0;
    } else {
      char *path = argv[2];
      err = afc_remove_path(client, path);
      if (err == NULL) {
        printf("Path removed successfully\n");
      } else {
        fprintf(stderr, "Failed to remove path: [%d] %s\n", err->code,
                err->message);
        idevice_error_free(err);
        success = 0;
      }
    }
  } else if (strcmp(command, "remove_all") == 0) {
    if (argc < 3) {
      printf("Error: Missing path argument\n");
      success = 0;
    } else {
      char *path = argv[2];
      err = afc_remove_path_and_contents(client, path);
      if (err == NULL) {
        printf("Path and contents removed successfully\n");
      } else {
        fprintf(stderr, "Failed to remove path and contents: [%d] %s\n",
                err->code, err->message);
        idevice_error_free(err);
        success = 0;
      }
    }
  } else if (strcmp(command, "info") == 0) {
    if (argc < 3) {
      printf("Error: Missing path argument\n");
      success = 0;
    } else {
      char *path = argv[2];
      AfcFileInfo info = {0};
      err = afc_get_file_info(client, path, &info);
      if (err == NULL) {
        print_file_info(&info);
        afc_file_info_free(&info);
      } else {
        fprintf(stderr, "Failed to get file info: [%d] %s\n", err->code,
                err->message);
        idevice_error_free(err);
        success = 0;
      }
    }
  } else if (strcmp(command, "device_info") == 0) {
    AfcDeviceInfo info = {0};
    err = afc_get_device_info(client, &info);
    if (err == NULL) {
      print_device_info(&info);
      afc_device_info_free(&info);
    } else {
      fprintf(stderr, "Failed to get device info: [%d] %s\n", err->code,
              err->message);
      idevice_error_free(err);
      success = 0;
    }
  } else {
    printf("Unknown command: %s\n", command);
    print_usage();
    success = 0;
  }

  // Cleanup
  afc_client_free(client);
  return success ? 0 : 1;
}
