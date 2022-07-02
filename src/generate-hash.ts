import * as crypto from 'crypto';

function generateHash(): string {
  const hash = crypto.randomBytes(20).toString('hex');
  return hash.slice(0, 4);
}

export default generateHash;
