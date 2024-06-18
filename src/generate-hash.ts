import * as crypto from 'crypto';

const generateRandomLetter = () => {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz';
  return alphabet[Math.floor(Math.random() * alphabet.length)];
}

function generateHash(): string {
  const hash: string = crypto.randomBytes(20).toString('hex');
  return `${generateRandomLetter()}${hash.slice(0, 4)}`;
}

export default generateHash;
