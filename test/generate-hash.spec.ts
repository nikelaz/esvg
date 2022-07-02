import generateHash from '../src/generate-hash';

describe('generateHash()', () => {
  test('returns a 4 symbol string hash', () => {
    const hash: string = generateHash();
    expect(hash).toHaveLength(4);
  });
});
