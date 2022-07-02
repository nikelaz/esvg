import SVGOAdapter from '../src/svgo-adapter';

/**
 * The SVGO plugin is tested, therefore this test suite only
 * checks if the plugin configurations work correctly
 */
describe('optimize()', () => {
  test('removes dimensions', () => {
    const svg = '<svg width="10" height="10"/>'

    const expectedResult = '<svg viewBox="0 0 10 10"/>';
    
    const result = SVGOAdapter.optimize(svg);

    expect(result).toEqual(expectedResult);
  });

  test('prefixes ids and classNames', () => {
    const svg = '<svg viewBox="0 0 100 100"><clipPath id="myClip"><circle cx="40" cy="35" r="35"/></clipPath><path id="heart" d="M10,30 A20,20,0,0,1,50,30 A20,20,0,0,1,90,30 Q90,60,50,90 Q10,60,10,30 Z"/><use clip-path="url(#myClip)" href="#heart" fill="red"/></svg>';
    const result = SVGOAdapter.optimize(svg);
    
    expect(result).toContain('id=');
    expect(result).not.toContain('myClip');
  });
});
