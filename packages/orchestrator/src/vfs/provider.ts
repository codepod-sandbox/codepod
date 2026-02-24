/**
 * Interface for virtual filesystem providers.
 *
 * Providers handle synthetic mount points like /dev and /proc,
 * intercepting VFS operations before they reach the inode tree.
 */

export interface VirtualProvider {
  /** Read the contents of a file at the given subpath (relative to mount point). */
  readFile(subpath: string): Uint8Array;

  /** Write data to a file at the given subpath. */
  writeFile(subpath: string, data: Uint8Array): void;

  /** Check whether a file or directory exists at the given subpath. */
  exists(subpath: string): boolean;

  /** Return type and size information for the given subpath. */
  stat(subpath: string): { type: 'file' | 'dir'; size: number };

  /** List entries in a directory at the given subpath. */
  readdir(subpath: string): Array<{ name: string; type: 'file' | 'dir' }>;
}
