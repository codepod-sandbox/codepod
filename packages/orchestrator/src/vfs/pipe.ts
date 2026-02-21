/**
 * Pipe implementation for shell pipeline execution.
 *
 * A pipe provides a unidirectional byte channel between a write end and
 * a read end. Data written to the write end can be consumed from the
 * read end in FIFO order. This is the building block for shell pipelines
 * like `cat file | grep pattern | wc -l`.
 */

export interface PipeReadEnd {
  read(buf: Uint8Array): number;
  close(): void;
}

export interface PipeWriteEnd {
  write(data: Uint8Array): void;
  close(): void;
}

interface PipeBuffer {
  chunks: Uint8Array[];
  totalBytes: number;
  writeClosed: boolean;
  readClosed: boolean;
}

/**
 * Create a pipe returning [readEnd, writeEnd].
 *
 * Both ends share an internal buffer. The write end appends data;
 * the read end consumes it. When the write end is closed and the
 * buffer is drained, reads return 0 (EOF).
 */
export function createPipe(): [PipeReadEnd, PipeWriteEnd] {
  const shared: PipeBuffer = {
    chunks: [],
    totalBytes: 0,
    writeClosed: false,
    readClosed: false,
  };

  const readEnd: PipeReadEnd = {
    read(buf: Uint8Array): number {
      if (shared.totalBytes === 0) {
        return 0;
      }

      let bytesRead = 0;
      const requested = buf.byteLength;

      while (bytesRead < requested && shared.chunks.length > 0) {
        const chunk = shared.chunks[0];
        const available = chunk.byteLength;
        const needed = requested - bytesRead;

        if (available <= needed) {
          buf.set(chunk, bytesRead);
          bytesRead += available;
          shared.chunks.shift();
        } else {
          buf.set(chunk.subarray(0, needed), bytesRead);
          shared.chunks[0] = chunk.subarray(needed);
          bytesRead += needed;
        }
      }

      shared.totalBytes -= bytesRead;
      return bytesRead;
    },

    close(): void {
      shared.readClosed = true;
    },
  };

  const writeEnd: PipeWriteEnd = {
    write(data: Uint8Array): void {
      if (shared.writeClosed) {
        throw new Error('write to closed pipe');
      }
      if (data.byteLength === 0) {
        return;
      }
      const copy = new Uint8Array(data);
      shared.chunks.push(copy);
      shared.totalBytes += copy.byteLength;
    },

    close(): void {
      shared.writeClosed = true;
    },
  };

  return [readEnd, writeEnd];
}
