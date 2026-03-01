import { describe, it } from '@std/testing/bdd';
import { expect } from '@std/expect';
import { ProcessKernel } from '../kernel.js';

describe('ProcessKernel', () => {
  it('createPipe returns connected read/write ends', async () => {
    const kernel = new ProcessKernel();
    const { readFd, writeFd } = kernel.createPipe(/*callerPid=*/ 0);
    expect(readFd).toBeGreaterThanOrEqual(3);
    expect(writeFd).toBeGreaterThanOrEqual(3);
    expect(writeFd).toBe(readFd + 1);
    kernel.dispose();
  });

  it('closeFd closes pipe ends', () => {
    const kernel = new ProcessKernel();
    const { readFd, writeFd } = kernel.createPipe(0);
    kernel.closeFd(0, writeFd);
    kernel.closeFd(0, readFd);
    kernel.dispose();
  });

  it('getFdTarget returns the target for a given fd', () => {
    const kernel = new ProcessKernel();
    const { readFd } = kernel.createPipe(0);
    const target = kernel.getFdTarget(0, readFd);
    expect(target).not.toBeNull();
    expect(target!.type).toBe('pipe_read');
    kernel.dispose();
  });
});
