// @ts-ignore
import * as svgFlatten from 'svg-flatten';

class SVGFlattenAdapter {
  public static flatten(inputSvg: string) {
    return svgFlatten(inputSvg).pathify().value();
  }
}

export default SVGFlattenAdapter;
