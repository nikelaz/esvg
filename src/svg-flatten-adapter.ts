import * as svgFlatten from 'svg-flatten';
import SVG from './svg';

class SVGFlattenAdapter {
  public static flatten(inputSvg: SVG) {
    const flattenedSvgStr = svgFlatten(inputSvg.toString()).pathify().value();
    return new SVG(flattenedSvgStr);
  }
}

export default SVGFlattenAdapter;
