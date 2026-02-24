/**
 * PackageManager — manages WASI binary packages in the VFS.
 *
 * Stores WASI binaries at `/usr/share/pkg/bin/<name>.wasm` and
 * metadata at `/usr/share/pkg/packages.json`.
 */

import type { VfsLike } from '../vfs/vfs-like.js';
import type { PackagePolicy } from '../security.js';

const PKG_ROOT = '/usr/share/pkg';
const PKG_BIN = `${PKG_ROOT}/bin`;
const PKG_META = `${PKG_ROOT}/packages.json`;

/** Metadata for an installed package. */
export interface PackageInfo {
  name: string;
  url: string;
  size: number;
  installedAt: number;
}

/** Error thrown by PackageManager operations. */
export class PkgError extends Error {
  constructor(public code: string, message: string) {
    super(message);
    this.name = 'PkgError';
  }
}

/** Manages WASI binary packages stored in the VFS. */
export class PackageManager {
  private packages: Map<string, PackageInfo>;

  constructor(
    private vfs: VfsLike,
    private policy: PackagePolicy,
  ) {
    this.packages = new Map();
    // Create package directories and load existing metadata.
    this.vfs.withWriteAccess(() => {
      this.vfs.mkdirp(PKG_BIN);
    });
    this.loadMetadata();
  }

  /** Check whether a URL's host is allowed by the package policy. Throws PkgError if denied. */
  checkHost(sourceUrl: string): void {
    if (!this.policy.enabled) {
      throw new PkgError('E_PKG_DISABLED', 'Package installation is disabled');
    }
    if (this.policy.allowedHosts !== undefined) {
      const host = new URL(sourceUrl).hostname;
      if (!this.matchesHostList(host, this.policy.allowedHosts)) {
        throw new PkgError(
          'E_PKG_HOST_DENIED',
          `Host '${host}' is not in the allowed hosts list`,
        );
      }
    }
  }

  /** Install a WASI binary package into the VFS. */
  install(name: string, wasmBytes: Uint8Array, sourceUrl: string): void {
    if (!this.policy.enabled) {
      throw new PkgError('E_PKG_DISABLED', 'Package installation is disabled');
    }
    if (this.policy.allowedHosts !== undefined) {
      const host = new URL(sourceUrl).hostname;
      if (!this.matchesHostList(host, this.policy.allowedHosts)) {
        throw new PkgError(
          'E_PKG_HOST_DENIED',
          `Host '${host}' is not in the allowed hosts list`,
        );
      }
    }
    if (this.packages.has(name)) {
      throw new PkgError('E_PKG_EXISTS', `Package '${name}' is already installed`);
    }
    if (
      this.policy.maxPackageBytes !== undefined &&
      wasmBytes.byteLength > this.policy.maxPackageBytes
    ) {
      throw new PkgError(
        'E_PKG_TOO_LARGE',
        `Package size ${wasmBytes.byteLength} exceeds limit of ${this.policy.maxPackageBytes} bytes`,
      );
    }
    if (
      this.policy.maxInstalledPackages !== undefined &&
      this.packages.size >= this.policy.maxInstalledPackages
    ) {
      throw new PkgError(
        'E_PKG_LIMIT',
        `Maximum of ${this.policy.maxInstalledPackages} packages reached`,
      );
    }

    const info: PackageInfo = {
      name,
      url: sourceUrl,
      size: wasmBytes.byteLength,
      installedAt: Date.now(),
    };

    this.vfs.withWriteAccess(() => {
      this.vfs.writeFile(`${PKG_BIN}/${name}.wasm`, wasmBytes);
      this.packages.set(name, info);
      this.saveMetadata();
    });
  }

  /** Remove an installed package. */
  remove(name: string): void {
    if (!this.packages.has(name)) {
      throw new PkgError('E_PKG_NOT_FOUND', `Package '${name}' is not installed`);
    }

    this.vfs.withWriteAccess(() => {
      this.vfs.unlink(`${PKG_BIN}/${name}.wasm`);
      this.packages.delete(name);
      this.saveMetadata();
    });
  }

  /** List all installed packages. */
  list(): PackageInfo[] {
    return Array.from(this.packages.values());
  }

  /** Get info for a specific package, or null if not installed. */
  info(name: string): PackageInfo | null {
    return this.packages.get(name) ?? null;
  }

  /** Get the VFS path of an installed package's wasm binary, or null if not installed. */
  getWasmPath(name: string): string | null {
    if (!this.packages.has(name)) {
      return null;
    }
    return `${PKG_BIN}/${name}.wasm`;
  }

  // -- Private helpers --

  /** Check if a hostname matches any entry in a host pattern list. */
  private matchesHostList(host: string, list: string[]): boolean {
    for (const pattern of list) {
      if (pattern.startsWith('*.')) {
        const suffix = pattern.slice(2);
        if (
          host.endsWith(suffix) &&
          host.length > suffix.length &&
          host[host.length - suffix.length - 1] === '.'
        ) {
          return true;
        }
      } else if (host === pattern) {
        return true;
      }
    }
    return false;
  }

  /** Load metadata from VFS if it exists. */
  private loadMetadata(): void {
    try {
      const raw = this.vfs.readFile(PKG_META);
      const entries: PackageInfo[] = JSON.parse(new TextDecoder().decode(raw));
      for (const entry of entries) {
        this.packages.set(entry.name, entry);
      }
    } catch {
      // No metadata file yet — fresh install.
    }
  }

  /** Persist metadata to VFS. */
  private saveMetadata(): void {
    const data = JSON.stringify(Array.from(this.packages.values()), null, 2);
    this.vfs.writeFile(PKG_META, new TextEncoder().encode(data));
  }
}
