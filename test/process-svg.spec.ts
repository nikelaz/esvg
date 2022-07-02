import processSvg from '../src/process-svg';
import SVG from '../src/svg';
import sinon from 'sinon';
import SVGOAdapter from '../src/svgo-adapter';
import SVGFlattenAdapter from '../src/svg-flatten-adapter';

describe('processSvg()', () => {
  test('calls the correct methods to process the file', () => {
    const mockSvg = new SVG('<svg></svg>');
    sinon.spy(SVGOAdapter, 'optimize');
    sinon.spy(SVGFlattenAdapter, 'flatten');

    const optimizeSpy: any = SVGOAdapter.optimize;
    const flattenSpy: any = SVGFlattenAdapter.flatten;

    processSvg(mockSvg);

    expect(optimizeSpy.calledTwice).toBeTruthy();
    expect(flattenSpy.called).toBeTruthy();
  });
});