import SVG from './svg';
import SVGOAdapter from './svgo-adapter';
import SVGFlattenAdapter from './svg-flatten-adapter';
import removeTransforms from './remove-transforms';
import { Options } from './types/options';

function processSvg(inputSvg: SVG, options: Options): SVG {
  let svg = inputSvg;
  if (options.removeTransforms) {
    // Convert all shapes in the svg to paths
    svg = SVGFlattenAdapter.flatten(svg);

    // Remove transforms from the svg
    svg = removeTransforms(svg);
  }

  // Optimize the svg with svgo
  return SVGOAdapter.optimize(svg, true, options.pretty);;
}

export default processSvg;
