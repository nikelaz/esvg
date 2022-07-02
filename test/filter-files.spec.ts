import filterFiles from '../src/filter-files';

describe('filterFiles()', () => {
  test('returns only svg files from an array of filenames', () => {
    const input = [ 'x.jpg', 'y.svg', 'z.txt' ];
    const expectedResult = [ 'y.svg' ];
    const result = filterFiles(input);

    expect(result).toEqual(expectedResult);
  });
});